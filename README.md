# Sideways OTel 🦀

[![CI](https://github.com/iclassproinc/sideways-otel/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/iclassproinc/sideways-otel/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/sideways-otel.svg)](https://crates.io/crates/sideways-otel)
[![docs.rs](https://img.shields.io/docsrs/sideways-otel)](https://docs.rs/sideways-otel)

> *Observability from the side - because crabs walk sideways, and so should your telemetry.*

A production-ready Rust telemetry library that provides vendor-neutral **OpenTelemetry** tracing, metrics, and logs, exported via OTLP to any compatible backend - a local Collector, the [.NET Aspire dashboard](https://learn.microsoft.com/dotnet/aspire/fundamentals/dashboard/overview), or any hosted vendor that speaks OTLP.

> **Note:** This crate was built with substantial AI assistance (Claude Code) - design, implementation, and documentation were all done in collaboration with an AI agent, with human review and testing against real OTLP backends throughout. We're noting this in the interest of transparency.

## Features

- 🔭 **Vendor-Neutral OTLP Export** - Traces, metrics, and logs exported over gRPC to any OTLP-compatible backend
- 🚀 **One-Line Initialization** - Simple `init_telemetry()` call sets up everything
- 🔧 **Standard `OTEL_*` Env Vars** - Configure via the environment variables already defined by the OpenTelemetry spec
- 💪 **Graceful Degradation** - Continues running even if the OTLP endpoint is unavailable
- 📈 **Native OpenTelemetry Metrics** - No vendor-specific macros; sync and observable (async/callback) instruments come straight from the OTel metrics API
- 🧵 **Span Helpers** - Small, generic helpers for attaching attributes and events to the current span
- 🔍 **Health Check Filtering** - Automatically filters out noisy health check spans

## Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
sideways-otel = "0.4"
```

Initialize in your application:

```rust
use sideways_otel::{init_telemetry, TelemetryConfig};
use sideways_otel::prelude::*;
use tracing::info;

fn main() {
    // Load from environment variables
    let config = TelemetryConfig::from_env();
    let telemetry = init_telemetry(&config);

    // Use tracing as normal
    info!("Application started!");

    // Emit metrics using the native OpenTelemetry API
    let requests = counter("requests.handled");
    requests.add(1, &[KeyValue::new("status", "success")]);

    // ... your application code ...

    // Flush and shut down on exit (important!)
    telemetry.shutdown();
}
```

## Configuration

### Environment Variables

`TelemetryConfig::from_env()` reads the standard OpenTelemetry environment variables, plus a few additions for enabling/disabling individual signals:

```bash
# Service identity
OTEL_SERVICE_NAME=my-service
OTEL_RESOURCE_ATTRIBUTES=deployment.environment=production,team=platform

# OTLP export
OTEL_EXPORTER_OTLP_PROTOCOL=grpc              # "grpc" (default) or "http/protobuf"
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317   # 4317 for grpc, 4318 for http/protobuf
OTEL_EXPORTER_OTLP_HEADERS=x-api-key=some-secret,x-tenant=acme

# Enable/disable individual signals (default: true for all)
OTEL_TRACES_ENABLED=true
OTEL_METRICS_ENABLED=true
OTEL_LOGS_ENABLED=true

# Metrics export interval, in milliseconds (default: 60000)
OTEL_METRIC_EXPORT_INTERVAL=60000

# Logging level - full tracing_subscriber directive syntax is supported,
# including per-target overrides and "off":
RUST_LOG=warn,h2=off,hyper=off,tonic=off,opentelemetry=off,opentelemetry_sdk=off

# Enable JSON-formatted console logging (default: false)
JSON_LOGGING=false
```

`OTEL_EXPORTER_OTLP_HEADERS` is the mechanism for passing whatever a given backend needs for authentication (an API key, a tenant header, etc.) - the library itself has no knowledge of any particular vendor. See [`examples/vendor_backend.rs`](examples/vendor_backend.rs) for a worked, vendor-agnostic example (endpoint and header both come from environment variables, so it works unmodified against whichever OTLP-compatible backend you point it at).

Both `http://` and `https://` endpoints work for either protocol - `https://` automatically gets a TLS-enabled channel trusting Mozilla's bundled root store (via `tonic`'s `tls-webpki-roots`), so no extra configuration is needed to reach a public vendor endpoint.

`OTEL_EXPORTER_OTLP_PROTOCOL=http/protobuf` is useful where gRPC/HTTP2 gets blocked (some corporate proxies/gateways) or where a vendor only exposes an HTTP ingestion endpoint. When left unset, the exporter falls back to its protocol-specific default endpoint (`http://localhost:4317` for gRPC, `http://localhost:4318` for HTTP/protobuf) if `OTEL_EXPORTER_OTLP_ENDPOINT` also isn't set.

### Programmatic Configuration

```rust
use sideways_otel::TelemetryConfig;

let config = TelemetryConfig::builder()
    .service_name("my-service")
    .otlp_endpoint("http://localhost:4317")
    .with_otlp_header("x-api-key", "some-secret")
    .with_resource_attribute("deployment.environment", "production")
    .build();

let telemetry = sideways_otel::init_telemetry(&config);
```

## Metrics

Metrics are created and recorded through the native OpenTelemetry metrics API - there are no macros to import. `sideways_otel::prelude` provides `counter`/`histogram`/`up_down_counter`/`gauge` shortcuts, each scoped automatically to the service name passed to `init_telemetry` - no need to repeat it at every call site:

```rust
use sideways_otel::prelude::*;

let requests = counter("requests.handled");
requests.add(1, &[KeyValue::new("status", "success"), KeyValue::new("endpoint", "/users")]);

let latency = histogram("request.duration_ms");
latency.record(42.5, &[KeyValue::new("endpoint", "/users")]);

let queue_depth = up_down_counter("queue.depth");
queue_depth.add(1, &[KeyValue::new("queue", "emails")]);
```

These default to `u64` counters, `f64` histograms/gauges, and `i64` up/down counters - the common case. For a different numeric type, per-instrument description/unit, or a meter scoped to something other than the service name, drop down to `meter()` (or `opentelemetry::global::meter("some-scope")`) and its `u64_counter()`/`f64_histogram()`/etc. builders directly.

Create each instrument once (e.g. in a `OnceLock` or at startup) and reuse it - `meter()` is cheap to call repeatedly, but recreating the instrument itself on every call is wasted work. Concretely, avoid this:

```rust
use sideways_otel::prelude::*;

// BAD: calls counter("requests.handled") - and therefore builds a brand new
// Counter instrument - on every single request.
async fn handle_request() {
    let requests = counter("requests.handled");
    requests.add(1, &[KeyValue::new("status", "success")]);
}
```

and instead build the instrument once, then just record against the same handle every time - a `Counter`/`Histogram`/etc. is cheap to `.clone()` and safe to share across threads (`Send + Sync`), so a `OnceLock` behind a plain function works well without any framework-specific state:

```rust
use sideways_otel::prelude::*;
use std::sync::OnceLock;

fn requests_counter() -> &'static Counter<u64> {
    static REQUESTS: OnceLock<Counter<u64>> = OnceLock::new();
    REQUESTS.get_or_init(|| counter("requests.handled"))
}

async fn handle_request() {
    // Cheap on every call: no instrument creation, just a OnceLock read.
    requests_counter().add(1, &[KeyValue::new("status", "success")]);
}
```

The same pattern applies to `histogram`/`up_down_counter`/`gauge`/their observable equivalents, and to instruments built directly from `meter()`.

### Observable (Async) Metrics

For a value that's cheaper or more natural to *sample on demand* than to push on every change - a queue depth, open file descriptors, current pool size - use the observable variants instead. Rather than calling `.add()`/`.record()` yourself, you register a callback once and the SDK invokes it on every collection cycle:

```rust
use sideways_otel::prelude::*;

// The returned handle can be dropped immediately - the callback keeps
// firing for the life of the MeterProvider regardless, since registration
// happens against the SDK's meter pipeline, not the handle itself.
let _open_connections = observable_gauge("db.pool.open_connections", |observer| {
    observer.observe(current_pool_size() as f64, &[KeyValue::new("pool", "primary")]);
});

let _queue_depth = observable_up_down_counter("queue.depth", |observer| {
    observer.observe(current_queue_depth(), &[KeyValue::new("queue", "emails")]);
});

let _requests_total = observable_counter("requests.total", |observer| {
    observer.observe(current_request_count(), &[]);
});
# fn current_pool_size() -> i64 { 0 }
# fn current_queue_depth() -> i64 { 0 }
# fn current_request_count() -> u64 { 0 }
```

A callback can call `observer.observe(...)` more than once with different attributes, to report several related values (e.g. per-queue depths) from a single registration. There's no `observable_histogram` - the OTel spec doesn't define one, since a histogram's whole point is recording a distribution of individual measurements as they happen, not a single sampled value.

## Tracing

Once telemetry is initialized, use the standard `tracing` crate for distributed tracing.

### Basic Logging

```rust
use tracing::{info, warn, error};

info!("Application started");
warn!(user_id = 123, "Rate limit approaching");
error!(error = ?err, "Failed to process request");
```

### Instrumentation

`#[tracing::instrument]` wraps a function in a span - by default named after the function, with every argument recorded as a field (via `Debug`), and child of whatever span was active when it was called:

```rust
#[tracing::instrument]
async fn process_request(id: u64) {
    tracing::info!(request_id = id, "Processing request");
    // ... do work ...
}
```

#### Adding More Data to the Span

The attribute takes several options for going beyond the defaults - useful when the arguments alone don't tell the whole story, contain things you don't want recorded, or when a value is only known partway through the function:

```rust
#[tracing::instrument(
    name = "orders.process",        // override the span name (default: the function name)
    skip(order),                    // don't try to Debug-format `order` as a field
    fields(
        order.id = %order.id,       // %  -> format with Display
        order.total_cents = order.total_cents,
        order.status = tracing::field::Empty, // declared now, filled in once known
    ),
    err,                             // if this returns Err, record it as a field automatically
)]
async fn process_order(order: &Order) -> Result<(), OrderError> {
    charge_card(&order.id).await?;

    // Record a field that wasn't known when the span was created.
    tracing::Span::current().record("order.status", "shipped");

    Ok(())
}
```

A few things worth knowing:
- `skip(arg1, arg2, ...)` (or `skip_all`) excludes arguments from the span's fields - required for anything that isn't `Debug`, and good practice for large payloads or secrets you don't want in your telemetry backend.
- `fields(...)` adds fields beyond the function's arguments. Prefix a value with `%` to format it with `Display`, `?` for `Debug` (the default for argument-derived fields), or leave it bare for values that already implement `tracing::Value` (integers, bools, strings).
- `tracing::field::Empty` reserves a field slot for a value you don't have yet; call `tracing::Span::current().record("field_name", &value)` later in the function once you do. Fields *not* declared up front (in `fields(...)` or the argument list) can't be recorded this way.
- `err` (or `err(Debug)` to use `Debug` instead of `Display`) automatically records an `Err` return value as a field and emits an event for it - handy instead of manually calling [`record_error`](#span-attribute-helpers) at every fallible call site.
- `level = "debug"` (or `trace`/`warn`/`error`) controls the span's level; default is `info`.

#### Skipping or Limiting Which Arguments Become Fields

By default every argument is recorded as a field. Two ways to cut that down, depending on whether you want to exclude a few arguments or only include a few:

```rust
// Exclude specific arguments - the rest are still recorded as usual.
#[tracing::instrument(skip(password))]
async fn login(username: &str, password: &str) {
    // span fields: username. password is never touched.
}

// Exclude everything, then explicitly opt back in only what you want -
// useful when most arguments are large, sensitive, or just not `Debug`.
#[tracing::instrument(skip_all, fields(order.id = %order.id))]
async fn process_order(order: &Order, db: &DbPool) {
    // span fields: only order.id. Neither `order` (as Debug) nor `db` show up.
}
```

There's no separate "include-list" attribute - `skip_all` plus `fields(...)` *is* the include-list pattern. Anything not `Debug` (most connection pools, clients, etc.) must be skipped one way or the other, or the code won't compile.

### Manual Child Spans

`#[tracing::instrument]` covers the common case (one span per function), but you can also create a child span by hand inside an already-instrumented function - useful for naming a sub-step distinctly, or for spans that don't map onto a whole function. There are two styles, and which one you need depends on whether the work spans an `.await` point:

```rust
use tracing::Instrument;

#[tracing::instrument]
async fn process_order(order_id: &str) {
    // Start/end (guard) style: fine for synchronous work. The span ends
    // when `_guard` drops at the end of the block. Do NOT hold this guard
    // across an `.await` - see below for why.
    {
        let _guard = tracing::info_span!("order.validate").entered();
        tracing::info!("Validating order");
    }

    // Closure/future style: required once the work spans .await points.
    // `.instrument(span)` re-enters the span every time the future is
    // polled, which correctly handles the future being suspended and later
    // resumed on a different thread - something a plain guard can't do.
    async {
        tracing::info!("Charging card");
    }
    .instrument(tracing::info_span!("order.charge_card"))
    .await;
}
```

Both spans show up as children of `process_order` in whatever exports the trace - confirmed by the console span stack (`tracing-subscriber`'s `fmt` layer prints the active span stack colon-separated): `process_order{order_id="order-42"}:order.validate: ...` and `process_order{order_id="order-42"}:order.charge_card: ...`. `tracing-opentelemetry` builds its OTel parent/child span links from that exact same span-stack data, so the nesting carries through to your OTLP backend identically.

**Why the guard/`.await` distinction matters**: an `Entered` guard is tied to the current thread's span stack. If you hold one across an `.await`, the async runtime can suspend your future and run something else entirely on that thread while the span is still "active" - so unrelated work ends up nested under your span, or the span's duration includes time it wasn't actually doing anything. `.instrument()` avoids this by re-entering the span only while the future is actually being polled.

### Span Attribute Helpers

`sideways_otel::span` (re-exported from the prelude) provides generic helpers for enriching the *current* span with OpenTelemetry attributes and events, without reaching for `tracing_opentelemetry` directly:

```rust
use sideways_otel::prelude::*;

#[tracing::instrument]
async fn process_order(order_id: &str) {
    set_attribute(KeyValue::new("order.id", order_id.to_string()));

    if let Err(err) = charge_card(order_id).await {
        record_error(&err);
    }

    add_event("order.completed", [KeyValue::new("order.id", order_id.to_string())]);
}
```

### Health Check Filtering

The library automatically filters out health check-related spans to reduce noise:
- Spans from `tonic_health`
- Spans containing "health", "Health", or "Check"
- gRPC health check services

### Handling Export Failures

If the OTLP endpoint is unreachable (down collector, bad TLS cert, wrong auth), export failures happen inside `opentelemetry_sdk`'s background batch-export tasks - they never surface through this crate's API (no `Result`, no panic). Instead, `opentelemetry_sdk`'s `internal-logs` feature (on by default) emits them as ordinary `tracing` events, e.g. `BatchSpanProcessor.ExportError` at `target: "opentelemetry_sdk"`, level `ERROR`. Those flow through whatever subscriber is currently installed - by default, that's console output, filtered by `RUST_LOG` exactly like any other log line. A clean run with no `*.ExportError` lines means export is succeeding; if you see them, check your endpoint/TLS/auth configuration.

Sideways OTel filters these same events *out* of the OTel logs bridge specifically (any `target` starting with `opentelemetry`), so they still print to console but are never re-exported as OTel log records. Without that filter, an export failure's own diagnostic event would get bridged into an OTel log record and sent over the same broken connection, which fails the same way and logs another diagnostic event - a self-feeding loop of "failed to export" logs about failing to export. This only affects the logs bridge; console output is unaffected and still shows every `*.ExportError` line `RUST_LOG` lets through.

### Composing Your Own Subscriber

`init_telemetry` installs a global `tracing` subscriber for you, which means it can only be called once per process and leaves no room for adding your own layers (a Sentry layer, a custom filter, another exporter). If you need that, call `init_telemetry_layer` instead - it does everything `init_telemetry` does (propagation, OTLP export, the global tracer/meter providers) but hands back the composed layer instead of installing it, so you assemble the final subscriber yourself:

```rust,no_run
use sideways_otel::{init_telemetry_layer, TelemetryConfig};
use tracing_subscriber::prelude::*;

fn main() {
    let config = TelemetryConfig::from_env();
    let (telemetry, sideways_layer) = init_telemetry_layer(&config);

    tracing_subscriber::registry()
        .with(sideways_layer)
        // .with(my_own_layer)
        .init();

    tracing::info!("Application started");

    telemetry.shutdown();
}
```

This also works with `traces_enabled`/`metrics_enabled`/`logs_enabled` all set to `false` - `init_telemetry_layer` still returns a (console-only) layer to compose, and propagation plus the global tracer/meter providers are still wired up, so a consuming application can rely on sideways-otel purely for those while doing its own thing with the `tracing` subscriber.

## Context Propagation

`init_telemetry` installs a global context propagator by default - **W3C Trace Context** (`traceparent`/`tracestate`) plus **W3C Baggage** (`baggage`), matching the `OTEL_PROPAGATORS` spec default. This is what lets a trace stay connected across a process boundary (an outgoing HTTP call, a message queue, etc.) instead of starting a brand new, disconnected trace in the next service.

Only these two formats are supported - B3, Jaeger, and X-Ray propagation each require a separate crate (`opentelemetry-zipkin`, `opentelemetry-jaeger-propagator`, `opentelemetry-aws` respectively) that this crate doesn't depend on. Override via `OTEL_PROPAGATORS` (comma-separated: `tracecontext`, `baggage`, or `none` to disable) or `TelemetryConfig::builder().propagators(vec![...])`.

Propagation is independent of trace *export* - it's installed even if `OTEL_TRACES_ENABLED=false`, since correlating requests across services is useful regardless of whether this particular process is exporting spans.

To actually use it, you inject the current span's context into whatever "carrier" your transport uses (HTTP headers, gRPC metadata, a message's key-value attributes) on the way out, and extract it back into a `Context` on the way in. `opentelemetry::propagation::{Injector, Extractor}` are simple two/three-method traits - implement them for your transport's header type, or use an existing implementation like `opentelemetry-http`'s `HeaderInjector`/`HeaderExtractor` if you're already depending on the `http` crate:

```rust
use sideways_otel::prelude::*;
use opentelemetry::propagation::Injector;
use std::collections::HashMap;

struct HashMapCarrier<'a>(&'a mut HashMap<String, String>);

impl Injector for HashMapCarrier<'_> {
    fn set(&mut self, key: &str, value: String) {
        self.0.insert(key.to_string(), value);
    }
}

#[tracing::instrument]
async fn call_downstream_service() {
    let mut headers = HashMap::new();
    opentelemetry::global::get_text_map_propagator(|propagator| {
        propagator.inject_context(&tracing::Span::current().context(), &mut HashMapCarrier(&mut headers));
    });
    // ... send `headers` along with the outgoing request ...
}
```

Extracting on the receiving end is the mirror image - implement `Extractor` (`get`/`keys`) over the incoming request's headers, then:

```rust,ignore
let parent_cx = opentelemetry::global::get_text_map_propagator(|propagator| {
    propagator.extract(&incoming_headers_carrier)
});
if let Err(err) = tracing::Span::current().set_parent(parent_cx) {
    tracing::warn!(?err, "failed to attach incoming trace context");
}
```

`set_parent` (from `OpenTelemetrySpanExt`, re-exported in the prelude) makes the current span a child of whatever trace the incoming request was already part of, so the whole call chain shows up as one connected trace instead of two separate ones.

## Local Testing

Any OTLP-compatible collector works. Two easy options:

**OpenTelemetry Collector** (Docker):

```bash
docker run -p 4317:4317 -p 4318:4318 otel/opentelemetry-collector:latest
```

**.NET Aspire dashboard** (standalone, no .NET project required) exposes an OTLP/gRPC endpoint and a browser UI for traces, logs, and metrics:

```bash
docker run --rm -it -p 18888:18888 -p 18889:18889 \
  mcr.microsoft.com/dotnet/aspire-dashboard:latest
```

Then point your service at it:

```bash
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:18889 OTEL_SERVICE_NAME=my-service cargo run
```

Open `http://localhost:18888` (using the browser auth token printed to the container's console) to see traces, structured logs, and metrics.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for the clippy/testing conventions this repo expects PRs to follow.

## Publishing

This crate is published to [crates.io](https://crates.io/crates/sideways-otel).

Before publishing:

```bash
cargo test --all-features
cargo clippy --all-targets --all-features
```

Then:

```bash
cargo publish --dry-run   # Verify the package contents and metadata
cargo publish             # Actually publish
```

`cargo publish` requires you to be logged in (`cargo login`) with an account that has publish rights on the crate, and will refuse to publish if `Cargo.toml` metadata (`description`, `license`, `readme`, etc.) is incomplete.

## License

Dual-licensed under either of

- [Apache License, Version 2.0](LICENSE-APACHE)
- [MIT license](LICENSE-MIT)

at your option.

## Credits

Powered by [OpenTelemetry](https://opentelemetry.io/).
