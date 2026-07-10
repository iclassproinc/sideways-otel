use crate::{TelemetryConfig, TelemetryError};
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_sdk::Resource;
use tracing::Metadata;
use tracing_subscriber::layer::{Context as LayerContext, Filter, Layer, SubscriberExt};
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::{EnvFilter, Registry};

/// Get an `EnvFilter` from configuration.
fn get_env_filter(config: &TelemetryConfig) -> EnvFilter {
    EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(&config.rust_log))
        .unwrap_or_else(|e| {
            eprintln!(
                "⚠️  Failed to parse RUST_LOG filter: {e}. Using default 'info'"
            );
            EnvFilter::new("info")
        })
}

/// Initialize console-only logging without OTLP export.
pub fn init_console_logging(config: &TelemetryConfig) {
    let env_filter = get_env_filter(config);
    let subscriber = Registry::default();

    let result = if config.json_logging {
        let console_layer = tracing_subscriber::fmt::layer()
            .with_ansi(false)
            .with_timer(tracing_subscriber::fmt::time::UtcTime::rfc_3339())
            .json()
            .flatten_event(true)
            .with_target(true)
            .with_span_list(true)
            .with_filter(env_filter);
        tracing::subscriber::set_global_default(subscriber.with(console_layer))
    } else {
        let console_layer = tracing_subscriber::fmt::layer()
            .with_ansi(false)
            .with_filter(env_filter);
        tracing::subscriber::set_global_default(subscriber.with(console_layer))
    };

    match result {
        Ok(()) => eprintln!("✅ Console logging initialized"),
        Err(e) => eprintln!("❌ Failed to initialize console logging: {e}"),
    }
}

/// Custom filter to exclude health check spans from export.
struct HealthCheckFilter;

impl<S> Filter<S> for HealthCheckFilter
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    fn enabled(&self, meta: &Metadata<'_>, _cx: &LayerContext<'_, S>) -> bool {
        let target = meta.target();

        if target.starts_with("tonic_health") {
            return false;
        }

        if target.contains("grpc.health") || target.contains("Health") {
            return false;
        }

        let name = meta.name();
        if name.contains("health") || name.contains("Health") || name.contains("Check") {
            return false;
        }

        true
    }
}

/// Build the OTLP headers map from configuration, shared by traces, metrics,
/// and logs exporters.
pub(crate) fn otlp_headers(config: &TelemetryConfig) -> std::collections::HashMap<String, String> {
    config.otlp_headers.iter().cloned().collect()
}

/// Build an OTLP exporter for a given signal (traces/metrics/logs), honoring
/// [`crate::OtlpProtocol`]: gRPC (via tonic, with header/TLS support) or
/// HTTP/protobuf (via reqwest, with plain string headers). `$builder_ctor` is
/// an expression like `opentelemetry_otlp::SpanExporter::builder()`.
#[macro_export]
macro_rules! build_otlp_exporter {
    ($builder_ctor:expr, $config:expr, $headers:expr) => {{
        use opentelemetry_otlp::{WithExportConfig, WithHttpConfig, WithTonicConfig};
        match $config.otlp_protocol {
            $crate::OtlpProtocol::Grpc => {
                let mut builder = $builder_ctor
                    .with_tonic()
                    .with_metadata($crate::tracing::build_metadata($headers)?);
                if let Some(endpoint) = &$config.otlp_endpoint {
                    builder = builder.with_endpoint(endpoint);
                    if let Some(tls) = $crate::tracing::tls_config(endpoint) {
                        builder = builder.with_tls_config(tls);
                    }
                }
                builder.build()
            }
            $crate::OtlpProtocol::HttpProtobuf => {
                let mut builder = $builder_ctor.with_http().with_headers($headers.clone());
                if let Some(endpoint) = &$config.otlp_endpoint {
                    builder = builder.with_endpoint(endpoint);
                }
                builder.build()
            }
        }
        .map_err(|e| $crate::TelemetryError::ExporterBuild(e.to_string()))
    }};
}

/// Initialize OTLP trace export and optionally OTLP log export, wiring both
/// into a `tracing` subscriber alongside console logging.
///
/// Returns Ok with (`tracer_provider`, optional `logger_provider`) if the OTLP
/// exporters were built successfully, or Err otherwise. On error the caller
/// should fall back to [`init_console_logging`].
///
/// # Errors
///
/// Returns [`TelemetryError::ExporterBuild`] if the OTLP span/log exporters
/// fail to build, or [`TelemetryError::SubscriberInit`] if a global `tracing`
/// subscriber has already been installed.
pub fn init_otlp_tracing(
    config: &TelemetryConfig,
    resource: Resource,
) -> Result<
    (
        opentelemetry_sdk::trace::SdkTracerProvider,
        Option<opentelemetry_sdk::logs::SdkLoggerProvider>,
    ),
    TelemetryError,
> {
    let headers = otlp_headers(config);

    let span_exporter: opentelemetry_otlp::SpanExporter =
        crate::build_otlp_exporter!(opentelemetry_otlp::SpanExporter::builder(), config, &headers)?;

    let tracer_provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
        .with_batch_exporter(span_exporter)
        .with_resource(resource.clone())
        .build();

    opentelemetry::global::set_tracer_provider(tracer_provider.clone());
    let tracer = tracer_provider.tracer(config.service_name.clone());

    let logger_provider = if config.logs_enabled {
        let log_exporter: opentelemetry_otlp::LogExporter =
            crate::build_otlp_exporter!(opentelemetry_otlp::LogExporter::builder(), config, &headers)?;

        Some(
            opentelemetry_sdk::logs::SdkLoggerProvider::builder()
                .with_batch_exporter(log_exporter)
                .with_resource(resource)
                .build(),
        )
    } else {
        None
    };

    macro_rules! set_subscriber {
        ($console_layer:expr) => {{
            let env_filter = get_env_filter(config);
            let subscriber = Registry::default();
            let console = $console_layer.with_filter(env_filter);
            let telemetry = tracing_opentelemetry::layer()
                .with_tracer(tracer)
                .with_filter(HealthCheckFilter)
                .with_filter(tracing_subscriber::filter::LevelFilter::INFO);

            if let Some(ref lp) = logger_provider {
                let logs_filter = get_env_filter(config);
                let logs_layer =
                    opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge::new(lp)
                        .with_filter(logs_filter);
                tracing::subscriber::set_global_default(
                    subscriber.with(console).with(telemetry).with(logs_layer),
                )
                .map_err(|e| TelemetryError::SubscriberInit(e.to_string()))?;
            } else {
                tracing::subscriber::set_global_default(subscriber.with(console).with(telemetry))
                    .map_err(|e| TelemetryError::SubscriberInit(e.to_string()))?;
            }
        }};
    }

    if config.json_logging {
        set_subscriber!(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_timer(tracing_subscriber::fmt::time::UtcTime::rfc_3339())
                .json()
                .flatten_event(true)
                .with_target(true)
                .with_span_list(true)
        );
    } else {
        set_subscriber!(tracing_subscriber::fmt::layer().with_ansi(false));
    }

    tracing::info!("🦀 OTLP tracing initialized successfully");

    Ok((tracer_provider, logger_provider))
}

/// Convert plain string headers into gRPC metadata for the tonic exporter.
pub(crate) fn build_metadata(
    headers: &std::collections::HashMap<String, String>,
) -> Result<tonic::metadata::MetadataMap, TelemetryError> {
    let mut map = tonic::metadata::MetadataMap::new();
    for (key, value) in headers {
        let key = tonic::metadata::MetadataKey::from_bytes(key.as_bytes())
            .map_err(|e| TelemetryError::ExporterBuild(format!("invalid header key: {e}")))?;
        let value = tonic::metadata::MetadataValue::try_from(value.as_str())
            .map_err(|e| TelemetryError::ExporterBuild(format!("invalid header value: {e}")))?;
        map.insert(key, value);
    }
    Ok(map)
}

/// Build a `ClientTlsConfig` trusting Mozilla's bundled root store for
/// `https://` endpoints. Returns `None` for plain `http://` endpoints (e.g. a
/// local Collector) so the exporter doesn't attempt a TLS handshake against a
/// plaintext port.
pub(crate) fn tls_config(endpoint: &str) -> Option<tonic::transport::ClientTlsConfig> {
    endpoint
        .starts_with("https://")
        .then(|| tonic::transport::ClientTlsConfig::new().with_webpki_roots())
}
