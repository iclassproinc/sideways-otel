# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**Sideways OTel** is a production-ready Rust telemetry library providing vendor-neutral OpenTelemetry tracing, metrics, and logs, exported via OTLP (gRPC or HTTP/protobuf) to any compatible backend (a local Collector, the .NET Aspire dashboard, or any hosted vendor that speaks OTLP). It is the OpenTelemetry-native sibling of [`sideways`](https://github.com/iclassproinc/sideways), which is Datadog/StatsD-specific. Unlike `sideways`, this crate has **no vendor-specific code or helpers** - all backend configuration (endpoints, auth headers) flows through standard `OTEL_*` environment variables or the builder API, and no single hosted vendor is named or favored in code, examples, or docs beyond illustrating that the mechanism is generic.

## Build and Development Commands

### Building
```bash
cargo build
cargo build --release
cargo build --examples
```

### Testing
```bash
cargo test --all-features
cargo test --doc
```

### Linting
```bash
cargo clippy --all-targets --all-features
```
Lints are configured in `Cargo.toml` under `[lints.clippy]`: `clippy::all` and `clippy::pedantic` at `warn`, plus `unwrap_used`/`expect_used` at `warn` to keep panics out of a library that's meant to degrade gracefully. Keep this clean before committing - see [Important Implementation Details](#important-implementation-details) for the reasoning behind the specific allows.

### Local end-to-end testing
There's no mock backend in this repo - test against a real OTLP receiver:
```bash
# OpenTelemetry Collector
docker run -p 4317:4317 -p 4318:4318 otel/opentelemetry-collector:latest

# .NET Aspire dashboard (has a browser UI for traces/logs/metrics)
docker run --rm -it -p 18888:18888 -p 18889:18889 mcr.microsoft.com/dotnet/aspire-dashboard:latest
```
Then run the example against it:
```bash
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:18889 OTEL_SERVICE_NAME=my-service cargo run --example basic
```
A clean exit with no `BatchSpanProcessor.ExportError` / `BatchLogProcessor.ExportError` lines means export succeeded.

`examples/vendor_backend.rs` exercises the HTTPS/TLS + auth-header path against a real hosted OTLP endpoint. It takes the endpoint, header name, and header value entirely from environment variables (`OTLP_ENDPOINT`/`OTLP_HEADER_NAME`/`OTLP_HEADER_VALUE`) rather than hardcoding any vendor, so it can be pointed at whichever backend you're validating against - use it when validating changes to `tls_config()`/`build_otlp_exporter!`/header handling. A gRPC `Unauthenticated` error (not `UnknownIssuer`/connection errors) means TLS and transport are working correctly and only the credential itself is invalid/missing, which is expected without a real API key.

### Publishing
Publishing is automated via `.github/workflows/publish.yml`: pushing a `vX.Y.Z` tag (that's reachable from `main` and matches `Cargo.toml`'s version) runs the test/clippy gate, then `cargo publish` and creates a GitHub release. Before tagging:
```bash
cargo test --all-features
cargo clippy --all-targets --all-features
cargo publish --dry-run  # Test the publishing process
```
Also bump the version in **`README.md`'s Quick Start** (`sideways-otel = "X.Y"` under `[dependencies]`) to match - it's a plain string, not something crates.io or the CI badges keep in sync automatically.

## Architecture

### Module Structure

1. **`src/lib.rs`** - Main entry point with configuration and initialization
   - `TelemetryConfig` - Configuration struct with builder pattern and environment-based loading (standard `OTEL_*` vars)
   - `init_telemetry()` - Single initialization function that sets up traces, metrics, and logs
   - `Telemetry` struct - Return value holding all three providers; must be kept alive and `.shutdown()` on exit

2. **`src/resource.rs`** - Builds the OpenTelemetry `Resource` (service name + extra attributes) shared by traces, metrics, and logs.

3. **`src/tracing.rs`** - OTLP trace + log export
   - Builds `SpanExporter`/`LogExporter` via the `build_otlp_exporter!` macro (defined here, `#[macro_export]`ed so `metrics.rs` can reuse it), which branches on `TelemetryConfig::otlp_protocol` to use either `.with_tonic()` (gRPC) or `.with_http()` (HTTP/protobuf)
   - `HealthCheckFilter` - Custom filter to exclude health check spans (tonic_health, grpc.health, etc.)
   - `otlp_headers()` - shared helper turning `TelemetryConfig::otlp_headers` into a `HashMap`; `build_metadata()` further converts that into gRPC `MetadataMap` (HTTP transport just clones the `HashMap` directly, no conversion needed)
   - `tls_config()` - returns a `ClientTlsConfig` trusting Mozilla's webpki roots for `https://` gRPC endpoints, `None` for plain `http://` (see TLS note below)
   - Supports both full OTLP tracing and console-only logging fallback

4. **`src/metrics.rs`** - OTLP metric export
   - Native `opentelemetry_sdk::metrics` API: `MetricExporter` (built via the same `build_otlp_exporter!` macro) + `PeriodicReader` + `SdkMeterProvider`
   - Registers the meter provider globally so `opentelemetry::global::meter()` works anywhere

5. **`src/span.rs`** - Generic helpers (`set_attribute`, `set_attributes`, `add_event`, `record_error`) for enriching the *current* `tracing` span with OTel attributes/events without reaching for `tracing_opentelemetry` directly.

6. **`src/prelude.rs`** - Convenience module re-exporting `opentelemetry::{global, KeyValue}`, metric instrument types, `OpenTelemetrySpanExt`, and the `span` helpers, plus a `meter()` shortcut (auto-scoped to the configured service name), `counter()`/`histogram()`/`up_down_counter()`/`gauge()` shortcuts for the default numeric type per synchronous instrument kind, and `observable_counter()`/`observable_up_down_counter()`/`observable_gauge()` for the async/callback-based equivalents (no `observable_histogram` - the OTel spec doesn't define one).
   - **TODO**: the instrument shortcuts still don't cover the alternate numeric types (`f64_counter`, `u64_histogram`, `i64_gauge`, `f64_up_down_counter`) - add them as real use cases come up rather than pre-building the full matrix. See the TODO comment in `src/prelude.rs`.

### Key Design Patterns

**Vendor-Neutral by Construction**: There is no Honeycomb/Datadog/etc.-specific code anywhere in this crate. Authentication and routing for a specific backend is entirely a matter of setting `OTEL_EXPORTER_OTLP_ENDPOINT` and `OTEL_EXPORTER_OTLP_HEADERS` (or the equivalent builder methods) - the library has no knowledge of what backend is on the other end. Do not add vendor-specific config/helpers here; that belongs in the consuming application.

**One-Line Initialization**: `init_telemetry(&config)` sets up traces, metrics, and logs in one call, mirroring `sideways`.

**Graceful Degradation**: If the OTLP endpoint is unavailable, the library logs to stderr and falls back to console-only logging rather than crashing the application (see `lib.rs::init_telemetry` and `tracing.rs::init_console_logging`).

**No Metrics Macros**: Unlike `sideways` (which needs `cadence-macros` because StatsD has no native Rust ergonomics), OpenTelemetry's metrics API is already ergonomic - instruments are created once from a `Meter` and recorded against directly. `prelude::meter()` is the only convenience wrapper; do not add macro-based metric helpers.

**Instruments Are Created Once, Not Per-Call**: `counter()`/`histogram()`/etc. build a new instrument on every call - calling one inside a hot-path function (e.g. a request handler) recreates the instrument on every request, which is wasted work. When writing example code, docs, or code that calls these helpers from somewhere that runs repeatedly, use a `OnceLock` (or build the instrument once at startup) and reuse the handle - see the pattern in README.md's Metrics section. `Counter`/`Histogram`/`UpDownCounter`/`Gauge` are all cheap-to-`.clone()`, `Send + Sync`, `Arc`-backed handles, so a plain `static OnceLock<Counter<u64>>` works without needing any framework-specific state.

### Tracing Architecture

- Base: `Registry::default()` subscriber
- Console layer: Standard formatted logging (no ANSI colors, or JSON via `json_logging`)
- Telemetry layer: `tracing-opentelemetry` → OTLP span exporter, with health check filtering
- Logs layer (optional): `opentelemetry-appender-tracing` bridges `tracing` events into OTel log records
- All layers share the same `EnvFilter` for log level control (`RUST_LOG`)

### Metrics Architecture

- `MetricExporter` (OTLP/gRPC) wrapped in a `PeriodicReader` (interval configurable via `OTEL_METRIC_EXPORT_INTERVAL`, default 60s)
- `SdkMeterProvider` registered globally via `opentelemetry::global::set_meter_provider`
- `Telemetry::shutdown()` forces a final flush so short-lived processes don't lose the last export cycle

## Important Implementation Details

### Provider Lifecycle
The `Telemetry` struct returned by `init_telemetry()` **must** be:
- Kept alive for the application lifetime
- Shut down on application exit via `telemetry.shutdown()`, which flushes and shuts down the tracer, meter, and logger providers

Failure to shut down properly can result in lost telemetry, especially for short-lived processes (CLIs, Lambdas) where the process would otherwise exit before the periodic metrics reader's next scheduled export.

### Instrumentation Requirement
For distributed tracing to work, functions must use `#[tracing::instrument]`. Without it, spans won't be created.

### Header/Auth Configuration
All OTLP auth (API keys, tenant IDs, etc.) goes through `OTEL_EXPORTER_OTLP_HEADERS` (format: `key1=value1,key2=value2`) or `TelemetryConfig::builder().with_otlp_header(key, value)`. For gRPC, `tracing.rs::build_metadata` converts these into `tonic::metadata::MetadataMap`; for HTTP/protobuf, `WithHttpConfig::with_headers` takes the plain `HashMap` directly. This is the only place vendor auth requirements (e.g. Honeycomb's `x-honeycomb-team` header) should ever be mentioned, and only in documentation/examples (see `examples/vendor_backend.rs`) - never hardcoded into the library.

### TLS
`https://` endpoints require a `ClientTlsConfig` with an explicit root store - enabling `tonic`'s `tls-ring`/`tls-webpki-roots` Cargo features alone is **not** sufficient, because `opentelemetry-otlp` builds a bare `ClientTlsConfig::new()` (no roots) by default when it detects an `https://` scheme. `tracing.rs::tls_config()` builds the actual `ClientTlsConfig::new().with_webpki_roots()` and `build_otlp_exporter!` passes it in via `.with_tls_config(...)` whenever `otlp_endpoint` starts with `https://`. If this regresses, the symptom is a `TonicTracesClient`/`TonicLogsClient` export error containing `UnknownIssuer` rather than a connection error - that's a certificate validation failure, not an auth failure (an auth failure surfaces as gRPC `Unauthenticated`, *after* a successful TLS handshake). This only applies to the gRPC transport; the HTTP/protobuf transport's TLS is handled by `reqwest` itself.

### Tokio Runtime Requirement
`init_telemetry()` is a plain (non-`async`) function, but it still requires being called from inside an active Tokio runtime (e.g. `#[tokio::main]`, or after entering a `Runtime`) - both `opentelemetry_sdk`'s `rt-tokio` feature (batch processors) and the TLS-enabled tonic channel construction need a live reactor. Calling it from a bare `fn main()` with no runtime panics with "there is no reactor running". This is why every example uses `#[tokio::main] async fn main()` even though none of them actually `.await` `init_telemetry()` itself.

### Dependencies
- **opentelemetry / opentelemetry_sdk / opentelemetry-otlp**: 0.32.x - OTLP export over gRPC (tonic) and HTTP/protobuf (reqwest), covering traces, metrics, and logs
- **tracing-opentelemetry**: 0.33.x
- **tonic**: pinned to match `opentelemetry-otlp`'s gRPC stack; used directly in `tracing.rs` to build `MetadataMap` for headers and `ClientTlsConfig` for TLS. Requires the `tls-ring` (crypto backend) and `tls-webpki-roots` (root cert store) features - both are needed, not just one

## Configuration

### Environment Variables
- `OTEL_SERVICE_NAME` - Service name (default: `sideways-otel-service`)
- `OTEL_RESOURCE_ATTRIBUTES` - Extra resource attributes, `key1=value1,key2=value2` (standard OTel format)
- `OTEL_EXPORTER_OTLP_PROTOCOL` - `grpc` (default) or `http/protobuf`
- `OTEL_EXPORTER_OTLP_ENDPOINT` - OTLP endpoint. When unset, falls back to the exporter's own protocol-specific default (`http://localhost:4317` for gRPC, `http://localhost:4318` for HTTP/protobuf) - this is why `TelemetryConfig::otlp_endpoint` is `Option<String>` rather than a `String` with a hardcoded default
- `OTEL_EXPORTER_OTLP_HEADERS` - Extra headers for every export request, `key1=value1,key2=value2`
- `OTEL_TRACES_ENABLED` / `OTEL_METRICS_ENABLED` / `OTEL_LOGS_ENABLED` - Enable/disable each signal (default: true)
- `OTEL_METRIC_EXPORT_INTERVAL` - Metrics export interval in milliseconds (default: 60000)
- `RUST_LOG` - Log level filter (default: info). Full `tracing_subscriber::EnvFilter` directive syntax is supported natively (no extra parsing needed), including per-target overrides and `off`, e.g. `warn,h2=off,hyper=off,tonic=off,opentelemetry=off,opentelemetry_sdk=off`
- `JSON_LOGGING` - Enable JSON-formatted console logging (default: false)
