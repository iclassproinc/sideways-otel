use sideways_otel::prelude::*;
use sideways_otel::{init_telemetry, TelemetryConfig};

#[tokio::main]
async fn main() {
    let config = TelemetryConfig::from_env();
    let telemetry = init_telemetry(&config);

    // Sampled on demand at each collection cycle rather than pushed on every
    // change - the callback keeps firing for the life of the MeterProvider
    // even though we don't hold on to the returned handle.
    let _open_connections = observable_gauge("orders.open_connections", |observer| {
        observer.observe(3.0, &[KeyValue::new("pool", "primary")]);
    });

    process_order("order-42").await;

    telemetry.shutdown();
}

#[tracing::instrument]
async fn process_order(order_id: &str) {
    tracing::info!("Processing order");
    set_attribute(KeyValue::new("order.id", order_id.to_string()));

    let requests = counter("orders.processed");
    requests.add(1, &[KeyValue::new("status", "success")]);

    let latency = histogram("order.processing_duration_ms");
    latency.record(12.5, &[KeyValue::new("order.id", order_id.to_string())]);

    let queue_depth = up_down_counter("orders.queue_depth");
    queue_depth.add(-1, &[]);
}
