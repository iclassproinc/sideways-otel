//! Shows how to add a `tracing` layer of your own alongside sideways-otel's,
//! instead of letting `init_telemetry` install the global subscriber for you.
//!
//! `init_telemetry_layer` does the same setup as `init_telemetry` -
//! propagation, OTLP export, the global tracer/meter providers - but hands
//! back the combined layer instead of installing it, so you control the
//! final `Registry` and can compose in anything else your application
//! needs (a Sentry layer, a metrics-from-logs layer, an extra filter, ...).
//! This also works with `OTEL_TRACES_ENABLED=false`/`OTEL_METRICS_ENABLED=false`/
//! `OTEL_LOGS_ENABLED=false` all set - `init_telemetry_layer` still returns a
//! (console-only) layer to compose, and propagation plus the global
//! providers are still wired up.
//!
//! ```bash
//! cargo run --example custom_subscriber
//! ```

use sideways_otel::{init_telemetry_layer, TelemetryConfig};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::prelude::*;

#[tokio::main]
async fn main() {
    let config = TelemetryConfig::from_env();
    let (telemetry, sideways_layer) = init_telemetry_layer(&config);

    let event_count = Arc::new(AtomicUsize::new(0));
    let counting_layer = EventCountingLayer(event_count.clone());

    // Compose sideways-otel's layer with our own on the same Registry, then
    // install the result ourselves - the one thing init_telemetry does that
    // init_telemetry_layer leaves to the caller.
    tracing_subscriber::registry()
        .with(sideways_layer)
        .with(counting_layer)
        .init();

    tracing::info!("Application started");
    tracing::info!("Processing something");

    println!(
        "counting_layer observed {} tracing events",
        event_count.load(Ordering::Relaxed)
    );

    telemetry.shutdown();
}

/// A minimal custom layer standing in for whatever a consuming application
/// might want to add - a Sentry layer, a metrics-from-logs bridge, etc.
struct EventCountingLayer(Arc<AtomicUsize>);

impl<S> Layer<S> for EventCountingLayer
where
    S: tracing::Subscriber,
{
    fn on_event(&self, _event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}
