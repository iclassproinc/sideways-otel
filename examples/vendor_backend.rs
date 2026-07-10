//! Shows how to point sideways-otel at a backend that requires an auth
//! header on every OTLP request (most hosted vendors do this instead of, or
//! in addition to, network-level auth) plus resource attributes that show up
//! on every span/metric/log.
//!
//! This is generic, vendor-agnostic code: `otlp_endpoint` / `with_otlp_header`
//! / `with_resource_attribute` are plain builder methods with no knowledge of
//! any particular backend. The endpoint, header name, and header value below
//! are read from environment variables rather than hardcoded, so this same
//! example works unmodified against whichever OTLP-compatible vendor you
//! point it at - check your vendor's own OTLP ingestion docs for their
//! specific endpoint and required header name.
//!
//! ```bash
//! OTLP_ENDPOINT=https://otlp.example-vendor.com:443 \
//! OTLP_HEADER_NAME=x-api-key \
//! OTLP_HEADER_VALUE=$VENDOR_API_KEY \
//! cargo run --example vendor_backend
//! ```
//!
//! The equivalent env-var-only setup with no code at all - just the standard
//! `OTEL_*` variables read by `TelemetryConfig::from_env()` - is:
//! ```bash
//! OTEL_EXPORTER_OTLP_ENDPOINT=https://otlp.example-vendor.com:443 \
//! OTEL_EXPORTER_OTLP_HEADERS=x-api-key=$VENDOR_API_KEY \
//! OTEL_RESOURCE_ATTRIBUTES=deployment.environment=production,team=platform \
//! OTEL_SERVICE_NAME=my-service \
//! cargo run
//! ```

use sideways_otel::prelude::*;
use sideways_otel::{init_telemetry, TelemetryConfig};

// init_telemetry() must run inside a Tokio runtime: the OTLP gRPC exporter's
// TLS-enabled channel and the batch span/log processors are built on top of
// Tokio's reactor, even though init_telemetry() itself isn't an async fn.
#[tokio::main]
async fn main() {
    let endpoint =
        std::env::var("OTLP_ENDPOINT").unwrap_or_else(|_| "http://localhost:4317".to_string());
    let header_name = std::env::var("OTLP_HEADER_NAME").unwrap_or_else(|_| "x-api-key".to_string());
    let header_value = std::env::var("OTLP_HEADER_VALUE").unwrap_or_else(|_| "replace-me".to_string());

    let config = TelemetryConfig::builder()
        .service_name("my-service")
        .otlp_endpoint(endpoint)
        // Auth header the backend needs on every trace/metric/log export.
        // The name and value are entirely backend-specific - check your
        // vendor's OTLP ingestion docs for what they expect.
        .with_otlp_header(header_name, header_value)
        // Attached to every span, metric, and log record - handy for
        // distinguishing environments, teams, or regions in the backend UI.
        .with_resource_attribute("deployment.environment", "production")
        .with_resource_attribute("team", "platform")
        .build();

    let telemetry = init_telemetry(&config);

    tracing::info!("Application started");

    let requests = counter("requests.handled");
    requests.add(1, &[KeyValue::new("status", "success")]);

    telemetry.shutdown();
}
