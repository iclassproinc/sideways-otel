use crate::tracing::otlp_headers;
use crate::{TelemetryConfig, TelemetryError};
use opentelemetry_sdk::metrics::{PeriodicReader, SdkMeterProvider};
use opentelemetry_sdk::Resource;
use std::time::Duration;

/// Initialize OTLP metric export via a periodic reader and register the
/// resulting `SdkMeterProvider` as the global meter provider.
///
/// Metrics are created and recorded through the native OpenTelemetry metrics
/// API, e.g. `opentelemetry::global::meter("my-service")`.
///
/// # Errors
///
/// Returns [`TelemetryError::ExporterBuild`] if the OTLP metric exporter
/// (headers or gRPC channel) fails to build.
pub fn init_otlp_metrics(
    config: &TelemetryConfig,
    resource: Resource,
) -> Result<SdkMeterProvider, TelemetryError> {
    let headers = otlp_headers(config);

    let exporter: opentelemetry_otlp::MetricExporter = crate::build_otlp_exporter!(
        opentelemetry_otlp::MetricExporter::builder(),
        config,
        &headers
    )?;

    let reader = PeriodicReader::builder(exporter)
        .with_interval(Duration::from_millis(config.metrics_export_interval_ms))
        .build();

    let meter_provider = SdkMeterProvider::builder()
        .with_reader(reader)
        .with_resource(resource)
        .build();

    opentelemetry::global::set_meter_provider(meter_provider.clone());

    Ok(meter_provider)
}
