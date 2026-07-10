/// Prelude module for convenient imports.
///
/// ```rust
/// use sideways_otel::prelude::*;
/// ```
///
/// Unlike vendor-specific metrics libraries, OpenTelemetry metrics don't need
/// macros - instruments are created once and then recorded against directly.
/// This prelude re-exports the pieces needed to do that, plus a handful of
/// `counter`/`histogram`/`up_down_counter`/`gauge` shortcuts for the common
/// case where you don't need per-instrument descriptions or units.
///
/// TODO: these shortcuts only cover the default numeric type per instrument
/// kind (`u64` counter, `f64` histogram/gauge, `i64` up/down counter) and
/// skip the observable (async/callback-based) instruments entirely
/// (`u64_observable_counter`, `*_observable_gauge`,
/// `*_observable_up_down_counter`) and the alternate numeric types
/// (`f64_counter`, `u64_histogram`, `i64_gauge`, `f64_up_down_counter`).
/// Add wrappers for these as real use cases show up, rather than
/// pre-building the full matrix speculatively.
pub use opentelemetry::metrics::{Counter, Gauge, Histogram, Meter, UpDownCounter};
pub use opentelemetry::{global, KeyValue};
pub use tracing_opentelemetry::OpenTelemetrySpanExt;

pub use crate::span::{add_event, record_error, set_attribute, set_attributes};

use std::borrow::Cow;

/// Get (or create) the global `Meter` scoped to the service name configured
/// via [`crate::init_telemetry`].
///
/// ```rust,no_run
/// use sideways_otel::prelude::*;
///
/// let requests = meter().u64_counter("requests.handled").build();
/// requests.add(1, &[KeyValue::new("status", "success")]);
/// ```
#[must_use]
pub fn meter() -> Meter {
    global::meter(crate::configured_service_name())
}

/// Create a `u64` counter instrument for recording increasing values.
///
/// ```rust,no_run
/// use sideways_otel::prelude::*;
///
/// let requests = counter("requests.handled");
/// requests.add(1, &[KeyValue::new("status", "success")]);
/// ```
#[must_use]
pub fn counter(name: impl Into<Cow<'static, str>>) -> Counter<u64> {
    meter().u64_counter(name).build()
}

/// Create an `f64` histogram instrument for recording a distribution of
/// values (e.g. request durations).
///
/// ```rust,no_run
/// use sideways_otel::prelude::*;
///
/// let latency = histogram("request.duration_ms");
/// latency.record(42.5, &[KeyValue::new("endpoint", "/users")]);
/// ```
#[must_use]
pub fn histogram(name: impl Into<Cow<'static, str>>) -> Histogram<f64> {
    meter().f64_histogram(name).build()
}

/// Create an `i64` up/down counter instrument for values that both increase
/// and decrease (e.g. a queue depth).
///
/// ```rust,no_run
/// use sideways_otel::prelude::*;
///
/// let queue_depth = up_down_counter("queue.depth");
/// queue_depth.add(-1, &[KeyValue::new("queue", "emails")]);
/// ```
#[must_use]
pub fn up_down_counter(name: impl Into<Cow<'static, str>>) -> UpDownCounter<i64> {
    meter().i64_up_down_counter(name).build()
}

/// Create an `f64` gauge instrument for recording the current value of
/// something that isn't a sum (e.g. CPU usage, temperature).
///
/// ```rust,no_run
/// use sideways_otel::prelude::*;
///
/// let cpu_usage = gauge("cpu.usage_percent");
/// cpu_usage.record(45.2, &[KeyValue::new("host", "web-01")]);
/// ```
#[must_use]
pub fn gauge(name: impl Into<Cow<'static, str>>) -> Gauge<f64> {
    meter().f64_gauge(name).build()
}
