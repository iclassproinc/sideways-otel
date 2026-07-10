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
/// synchronous case, and `observable_counter`/`observable_up_down_counter`/
/// `observable_gauge` for the async/callback-based case (a value that's
/// cheaper or more natural to sample at collection time - queue depth, open
/// file descriptors - than to push on every change). There's no
/// `observable_histogram`: the `OTel` spec doesn't define one, since a
/// histogram's whole point is recording a distribution of individual
/// measurements as they happen.
///
/// TODO: these shortcuts only cover the default numeric type per instrument
/// kind (`u64` counter, `f64` histogram/gauge, `i64` up/down counter) and
/// skip the alternate numeric types (`f64_counter`, `u64_histogram`,
/// `i64_gauge`, `f64_up_down_counter`). Add wrappers for these as real use
/// cases show up, rather than pre-building the full matrix speculatively.
pub use opentelemetry::metrics::{
    AsyncInstrument, Counter, Gauge, Histogram, Meter, ObservableCounter, ObservableGauge,
    ObservableUpDownCounter, UpDownCounter,
};
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

/// Create a `u64` observable (async) counter: `callback` is invoked by the
/// SDK once per collection cycle rather than being called directly, and
/// should call `observer.observe(value, attributes)` for each value it wants
/// to report (more than once, with different attributes, if applicable).
///
/// ```rust,no_run
/// use sideways_otel::prelude::*;
///
/// let _requests_total = observable_counter("requests.total", |observer| {
///     observer.observe(current_request_count(), &[]);
/// });
/// # fn current_request_count() -> u64 { 0 }
/// ```
#[must_use]
pub fn observable_counter(
    name: impl Into<Cow<'static, str>>,
    callback: impl Fn(&dyn AsyncInstrument<u64>) + Send + Sync + 'static,
) -> ObservableCounter<u64> {
    meter()
        .u64_observable_counter(name)
        .with_callback(callback)
        .build()
}

/// Create an `i64` observable (async) up/down counter: `callback` is invoked
/// by the SDK once per collection cycle and should call
/// `observer.observe(value, attributes)` for each value it wants to report.
///
/// ```rust,no_run
/// use sideways_otel::prelude::*;
///
/// let _queue_depth = observable_up_down_counter("queue.depth", |observer| {
///     observer.observe(current_queue_depth(), &[KeyValue::new("queue", "emails")]);
/// });
/// # fn current_queue_depth() -> i64 { 0 }
/// ```
#[must_use]
pub fn observable_up_down_counter(
    name: impl Into<Cow<'static, str>>,
    callback: impl Fn(&dyn AsyncInstrument<i64>) + Send + Sync + 'static,
) -> ObservableUpDownCounter<i64> {
    meter()
        .i64_observable_up_down_counter(name)
        .with_callback(callback)
        .build()
}

/// Create an `f64` observable (async) gauge: `callback` is invoked by the
/// SDK once per collection cycle and should call
/// `observer.observe(value, attributes)` for each value it wants to report.
/// Prefer this over [`gauge`] whenever the current value is cheaper or more
/// natural to sample on demand (e.g. reading OS/process stats) than to push
/// on every change.
///
/// ```rust,no_run
/// use sideways_otel::prelude::*;
///
/// let _cpu_usage = observable_gauge("cpu.usage_percent", |observer| {
///     observer.observe(current_cpu_usage(), &[KeyValue::new("host", "web-01")]);
/// });
/// # fn current_cpu_usage() -> f64 { 0.0 }
/// ```
#[must_use]
pub fn observable_gauge(
    name: impl Into<Cow<'static, str>>,
    callback: impl Fn(&dyn AsyncInstrument<f64>) + Send + Sync + 'static,
) -> ObservableGauge<f64> {
    meter()
        .f64_observable_gauge(name)
        .with_callback(callback)
        .build()
}
