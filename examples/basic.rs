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

    // Start/end (guard) style child span - fine for synchronous work, but
    // must not be held across an .await point (see the next example for that).
    {
        let _guard = tracing::info_span!("order.validate").entered();
        tracing::info!("Validating order");
    } // span ends here, when the guard drops

    // Closure/future style child span, required once the work spans .await
    // points - holding an `Entered` guard across an await is a known
    // correctness footgun (the span can leak onto whatever else runs on that
    // thread while this task is suspended).
    use tracing::Instrument;
    async {
        tracing::info!("Charging card");
    }
    .instrument(tracing::info_span!("order.charge_card"))
    .await;

    let requests = counter("orders.processed");
    requests.add(1, &[KeyValue::new("status", "success")]);

    let latency = histogram("order.processing_duration_ms");
    latency.record(12.5, &[KeyValue::new("order.id", order_id.to_string())]);

    let queue_depth = up_down_counter("orders.queue_depth");
    queue_depth.add(-1, &[]);

    // Propagate the current trace context across a process boundary (e.g.
    // an outgoing HTTP request) - inject it into a carrier here, and the
    // receiving service extracts it the same way to continue the same trace.
    let mut carrier = std::collections::HashMap::new();
    opentelemetry::global::get_text_map_propagator(|propagator| {
        propagator.inject_context(&tracing::Span::current().context(), &mut HashMapCarrier(&mut carrier));
    });
    tracing::info!(?carrier, "Headers to send with the outgoing request");
}

struct HashMapCarrier<'a>(&'a mut std::collections::HashMap<String, String>);

impl opentelemetry::propagation::Injector for HashMapCarrier<'_> {
    fn set(&mut self, key: &str, value: String) {
        self.0.insert(key.to_string(), value);
    }
}
