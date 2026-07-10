# Sideways OTel 🦀

> *Observability from the side - because crabs walk sideways, and so should your telemetry.*

A production-ready Rust telemetry library that provides vendor-neutral **OpenTelemetry** tracing, metrics, and logs, exported via OTLP to any compatible backend - a local Collector, the [.NET Aspire dashboard](https://learn.microsoft.com/dotnet/aspire/fundamentals/dashboard/overview), or any hosted vendor that speaks OTLP.

> **Note:** This crate was built with substantial AI assistance (Claude Code) - design, implementation, and documentation were all done in collaboration with an AI agent, with human review and testing against real OTLP backends throughout. We're noting this in the interest of transparency.

## Features

- 🔭 **Vendor-Neutral OTLP Export** - Traces, metrics, and logs exported over gRPC to any OTLP-compatible backend
- 🚀 **One-Line Initialization** - Simple `init_telemetry()` call sets up everything
- 🔧 **Standard `OTEL_*` Env Vars** - Configure via the environment variables already defined by the OpenTelemetry spec
- 💪 **Graceful Degradation** - Continues running even if the OTLP endpoint is unavailable
- 📈 **Native OpenTelemetry Metrics** - No vendor-specific macros; instruments come straight from the OTel metrics API
- 🧵 **Span Helpers** - Small, generic helpers for attaching attributes and events to the current span
- 🔍 **Health Check Filtering** - Automatically filters out noisy health check spans

## Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
sideways-otel = "0.1"
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

Create each instrument once (e.g. in a `OnceLock` or at startup) and reuse it - `meter()` is cheap to call repeatedly, but recreating the instrument itself on every call is wasted work.

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

```rust
#[tracing::instrument]
async fn process_request(id: u64) {
    tracing::info!(request_id = id, "Processing request");
    // ... do work ...
}
```

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

Built by iClassPro team, powered by [OpenTelemetry](https://opentelemetry.io/).
