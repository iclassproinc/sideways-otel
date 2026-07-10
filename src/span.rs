//! Helpers for enriching the current `tracing` span with OpenTelemetry
//! attributes and events, without needing to reach for
//! `tracing_opentelemetry` directly.

use opentelemetry::KeyValue;
use tracing_opentelemetry::OpenTelemetrySpanExt;

/// Set a single attribute on the current span.
///
/// ```rust,no_run
/// use sideways_otel::span::set_attribute;
/// use opentelemetry::KeyValue;
///
/// set_attribute(KeyValue::new("user.id", 42));
/// ```
pub fn set_attribute(attribute: KeyValue) {
    let KeyValue { key, value, .. } = attribute;
    tracing::Span::current().set_attribute(key, value);
}

/// Set multiple attributes on the current span at once.
///
/// ```rust,no_run
/// use sideways_otel::span::set_attributes;
/// use opentelemetry::KeyValue;
///
/// set_attributes([
///     KeyValue::new("user.id", 42),
///     KeyValue::new("tenant.id", "acme"),
/// ]);
/// ```
pub fn set_attributes(attributes: impl IntoIterator<Item = KeyValue>) {
    let span = tracing::Span::current();
    for KeyValue { key, value, .. } in attributes {
        span.set_attribute(key, value);
    }
}

/// Record an event (a timestamped log-like entry) on the current span.
///
/// ```rust,no_run
/// use sideways_otel::span::add_event;
/// use opentelemetry::KeyValue;
///
/// add_event("cache.miss", [KeyValue::new("cache.key", "user:42")]);
/// ```
pub fn add_event(name: impl Into<std::borrow::Cow<'static, str>>, attributes: impl IntoIterator<Item = KeyValue>) {
    tracing::Span::current().add_event(name, attributes.into_iter().collect());
}

/// Mark the current span as errored, attaching the error's `Display` output
/// as the standard `exception.message` attribute.
///
/// ```rust,no_run
/// use sideways_otel::span::record_error;
///
/// if let Err(err) = std::fs::read_to_string("config.toml") {
///     record_error(&err);
/// }
/// ```
pub fn record_error(error: &dyn std::error::Error) {
    let span = tracing::Span::current();
    span.set_attribute("error", true);
    span.add_event(
        "exception",
        vec![KeyValue::new("exception.message", error.to_string())],
    );
}
