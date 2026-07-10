use crate::{PropagatorKind, TelemetryConfig};
use opentelemetry::propagation::TextMapCompositePropagator;
use opentelemetry_sdk::propagation::{BaggagePropagator, TraceContextPropagator};

/// Build and install the global `TextMapPropagator` from
/// [`TelemetryConfig::propagators`].
///
/// This governs how trace context (and optionally baggage) is
/// injected/extracted across process boundaries - e.g. the `traceparent`
/// HTTP header - via `opentelemetry::global::get_text_map_propagator`. It is
/// independent of whether trace *export* is enabled: propagation still
/// matters for correlating incoming/outgoing requests even if this service
/// isn't itself exporting spans.
pub(crate) fn init_propagator(config: &TelemetryConfig) {
    let propagators: Vec<Box<dyn opentelemetry::propagation::TextMapPropagator + Send + Sync>> =
        config
            .propagators
            .iter()
            .map(|kind| -> Box<dyn opentelemetry::propagation::TextMapPropagator + Send + Sync> {
                match kind {
                    PropagatorKind::TraceContext => Box::new(TraceContextPropagator::new()),
                    PropagatorKind::Baggage => Box::new(BaggagePropagator::new()),
                }
            })
            .collect();

    opentelemetry::global::set_text_map_propagator(TextMapCompositePropagator::new(propagators));
}
