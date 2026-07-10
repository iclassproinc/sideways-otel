//! # Sideways `OTel` 🦀
//!
//! > *Observability from the side - because crabs walk sideways, and so should your telemetry.*
//!
//! A production-ready telemetry library for Rust services that provides
//! vendor-neutral **OpenTelemetry** tracing, metrics, and logs, exported via
//! OTLP to any compatible backend (a local Collector, Honeycomb, or anything
//! else that speaks OTLP).
//!
//! ## Features
//!
//! - Easy one-line initialization for traces, metrics, and logs
//! - Graceful degradation when the OTLP endpoint is unavailable
//! - Environment-based configuration using standard `OTEL_*` variables
//! - Health check filtering to reduce noise
//! - Native OpenTelemetry metrics API - no vendor-specific macros required
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use sideways_otel::prelude::*;
//! use sideways_otel::{init_telemetry, TelemetryConfig};
//!
//! fn main() {
//!     let config = TelemetryConfig::from_env();
//!     let telemetry = init_telemetry(&config);
//!
//!     tracing::info!("Application started");
//!
//!     let requests = counter("requests.handled");
//!     requests.add(1, &[KeyValue::new("status", "success")]);
//!
//!     telemetry.shutdown();
//! }
//! ```

pub mod metrics;
pub mod prelude;
pub mod resource;
pub mod span;
pub mod tracing;

use std::env;
use std::sync::OnceLock;
use thiserror::Error;

static SERVICE_NAME: OnceLock<String> = OnceLock::new();

/// The service name configured at [`init_telemetry`] time, used as the
/// instrumentation scope name for [`prelude::meter`]. Falls back to
/// `TelemetryConfig::default().service_name` if telemetry hasn't been
/// initialized yet (e.g. in tests).
pub(crate) fn configured_service_name() -> &'static str {
    SERVICE_NAME
        .get()
        .map_or("sideways-otel-service", String::as_str)
}

/// Which OTLP wire protocol to export over.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OtlpProtocol {
    /// OTLP/gRPC (the default). Typically port 4317.
    #[default]
    Grpc,
    /// OTLP/HTTP with protobuf-encoded bodies. Typically port 4318. Useful
    /// where gRPC/HTTP2 is blocked (some proxies/gateways) or where a vendor
    /// only exposes an HTTP ingestion endpoint.
    HttpProtobuf,
}

#[derive(Debug, Error)]
pub enum TelemetryError {
    #[error("OpenTelemetry tracing disabled via OTEL_TRACES_ENABLED=false")]
    TracingDisabled,

    #[error("OpenTelemetry metrics disabled via OTEL_METRICS_ENABLED=false")]
    MetricsDisabled,

    #[error("Failed to set global subscriber: {0}")]
    SubscriberInit(String),

    #[error("Failed to build OTLP exporter: {0}")]
    ExporterBuild(String),
}

/// Configuration for telemetry initialization.
#[derive(Debug, Clone)]
pub struct TelemetryConfig {
    /// Service name, sent as the `service.name` resource attribute.
    pub service_name: String,
    /// Extra resource attributes attached to every span/metric/log.
    pub resource_attributes: Vec<(String, String)>,

    /// Enable/disable trace export (default: true).
    pub traces_enabled: bool,
    /// Enable/disable metrics export (default: true).
    pub metrics_enabled: bool,
    /// Enable/disable log export (default: true).
    pub logs_enabled: bool,

    /// Which OTLP wire protocol to use (default: [`OtlpProtocol::Grpc`]).
    pub otlp_protocol: OtlpProtocol,
    /// OTLP endpoint, e.g. `http://localhost:4317`. When `None`, the
    /// exporter falls back to the protocol-specific default
    /// (`http://localhost:4317` for gRPC, `http://localhost:4318` for
    /// HTTP/protobuf).
    pub otlp_endpoint: Option<String>,
    /// Extra headers sent with every OTLP export request (e.g. API keys).
    pub otlp_headers: Vec<(String, String)>,

    /// `RUST_LOG` filter for both console output and the `OTel` logs bridge.
    pub rust_log: String,
    /// Enable JSON-formatted console logging (default: false).
    pub json_logging: bool,

    /// Metrics export interval, in milliseconds (default: 60000).
    pub metrics_export_interval_ms: u64,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            service_name: "sideways-otel-service".to_string(),
            resource_attributes: Vec::new(),
            traces_enabled: true,
            metrics_enabled: true,
            logs_enabled: true,
            otlp_protocol: OtlpProtocol::default(),
            otlp_endpoint: None,
            otlp_headers: Vec::new(),
            rust_log: "info".to_string(),
            json_logging: false,
            metrics_export_interval_ms: 60_000,
        }
    }
}

impl TelemetryConfig {
    /// Load configuration from standard `OTEL_*` environment variables.
    #[must_use]
    pub fn from_env() -> Self {
        let mut config = Self::default();

        if let Ok(name) = env::var("OTEL_SERVICE_NAME") {
            config.service_name = name;
        }
        if let Ok(attrs) = env::var("OTEL_RESOURCE_ATTRIBUTES") {
            config.resource_attributes = Self::parse_pairs(&attrs, '=');
        }

        if let Ok(enabled) = env::var("OTEL_TRACES_ENABLED")
            && enabled.eq_ignore_ascii_case("false")
        {
            config.traces_enabled = false;
        }
        if let Ok(enabled) = env::var("OTEL_METRICS_ENABLED")
            && enabled.eq_ignore_ascii_case("false")
        {
            config.metrics_enabled = false;
        }
        if let Ok(enabled) = env::var("OTEL_LOGS_ENABLED")
            && enabled.eq_ignore_ascii_case("false")
        {
            config.logs_enabled = false;
        }

        if let Ok(protocol) = env::var("OTEL_EXPORTER_OTLP_PROTOCOL") {
            match protocol.as_str() {
                "grpc" => config.otlp_protocol = OtlpProtocol::Grpc,
                "http/protobuf" => config.otlp_protocol = OtlpProtocol::HttpProtobuf,
                other => eprintln!(
                    "⚠️  Unsupported OTEL_EXPORTER_OTLP_PROTOCOL '{other}' (expected 'grpc' or 'http/protobuf'), defaulting to grpc"
                ),
            }
        }
        if let Ok(endpoint) = env::var("OTEL_EXPORTER_OTLP_ENDPOINT") {
            config.otlp_endpoint = Some(endpoint);
        }
        if let Ok(headers) = env::var("OTEL_EXPORTER_OTLP_HEADERS") {
            config.otlp_headers = Self::parse_pairs(&headers, '=');
        }

        if let Ok(rust_log) = env::var("RUST_LOG") {
            config.rust_log = rust_log;
        }
        if let Ok(enabled) = env::var("JSON_LOGGING")
            && enabled.eq_ignore_ascii_case("true")
        {
            config.json_logging = true;
        }
        if let Ok(interval) = env::var("OTEL_METRIC_EXPORT_INTERVAL")
            && let Ok(ms) = interval.parse()
        {
            config.metrics_export_interval_ms = ms;
        }

        config
    }

    /// Parse a comma-separated list of `key<sep>value` pairs, the format
    /// used by `OTEL_RESOURCE_ATTRIBUTES` and `OTEL_EXPORTER_OTLP_HEADERS`.
    fn parse_pairs(raw: &str, sep: char) -> Vec<(String, String)> {
        raw.split(',')
            .filter_map(|pair| {
                let mut parts = pair.trim().splitn(2, sep);
                let key = parts.next()?.trim();
                let value = parts.next()?.trim();
                if key.is_empty() {
                    None
                } else {
                    Some((key.to_string(), value.to_string()))
                }
            })
            .collect()
    }

    /// Create a builder for custom configuration.
    #[must_use]
    pub fn builder() -> TelemetryConfigBuilder {
        TelemetryConfigBuilder::default()
    }
}

/// Builder for `TelemetryConfig`.
#[derive(Debug, Default)]
pub struct TelemetryConfigBuilder {
    config: TelemetryConfig,
}

impl TelemetryConfigBuilder {
    #[must_use]
    pub fn service_name(mut self, name: impl Into<String>) -> Self {
        self.config.service_name = name.into();
        self
    }

    #[must_use]
    pub fn resource_attributes(mut self, attributes: Vec<(String, String)>) -> Self {
        self.config.resource_attributes = attributes;
        self
    }

    #[must_use]
    pub fn with_resource_attribute(
        mut self,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        self.config
            .resource_attributes
            .push((key.into(), value.into()));
        self
    }

    #[must_use]
    pub fn traces_enabled(mut self, enabled: bool) -> Self {
        self.config.traces_enabled = enabled;
        self
    }

    #[must_use]
    pub fn metrics_enabled(mut self, enabled: bool) -> Self {
        self.config.metrics_enabled = enabled;
        self
    }

    #[must_use]
    pub fn logs_enabled(mut self, enabled: bool) -> Self {
        self.config.logs_enabled = enabled;
        self
    }

    #[must_use]
    pub fn otlp_protocol(mut self, protocol: OtlpProtocol) -> Self {
        self.config.otlp_protocol = protocol;
        self
    }

    #[must_use]
    pub fn otlp_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.config.otlp_endpoint = Some(endpoint.into());
        self
    }

    #[must_use]
    pub fn otlp_headers(mut self, headers: Vec<(String, String)>) -> Self {
        self.config.otlp_headers = headers;
        self
    }

    #[must_use]
    pub fn with_otlp_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.config.otlp_headers.push((key.into(), value.into()));
        self
    }

    #[must_use]
    pub fn rust_log(mut self, filter: impl Into<String>) -> Self {
        self.config.rust_log = filter.into();
        self
    }

    #[must_use]
    pub fn json_logging(mut self, enabled: bool) -> Self {
        self.config.json_logging = enabled;
        self
    }

    #[must_use]
    pub fn metrics_export_interval_ms(mut self, ms: u64) -> Self {
        self.config.metrics_export_interval_ms = ms;
        self
    }

    #[must_use]
    pub fn build(self) -> TelemetryConfig {
        self.config
    }
}

/// Telemetry components that need to be kept alive for the duration of the
/// application and flushed/shutdown on exit.
pub struct Telemetry {
    pub tracer_provider: Option<opentelemetry_sdk::trace::SdkTracerProvider>,
    pub meter_provider: Option<opentelemetry_sdk::metrics::SdkMeterProvider>,
    pub logger_provider: Option<opentelemetry_sdk::logs::SdkLoggerProvider>,
}

impl Telemetry {
    /// Flush and shut down all initialized providers. Call this before the
    /// application exits to avoid losing buffered telemetry.
    pub fn shutdown(&self) {
        if let Some(tp) = &self.tracer_provider
            && let Err(err) = tp.shutdown()
        {
            eprintln!("⚠️  Sideways OTel: tracer provider shutdown failed: {err}");
        }
        if let Some(mp) = &self.meter_provider
            && let Err(err) = mp.shutdown()
        {
            eprintln!("⚠️  Sideways OTel: meter provider shutdown failed: {err}");
        }
        if let Some(lp) = &self.logger_provider
            && let Err(err) = lp.shutdown()
        {
            eprintln!("⚠️  Sideways OTel: logger provider shutdown failed: {err}");
        }
    }
}

/// Describe the OTLP endpoint for log output: the configured endpoint if
/// set, otherwise the protocol's default.
fn describe_endpoint(config: &TelemetryConfig) -> &str {
    config.otlp_endpoint.as_deref().unwrap_or(match config.otlp_protocol {
        OtlpProtocol::Grpc => "http://localhost:4317 (grpc default)",
        OtlpProtocol::HttpProtobuf => "http://localhost:4318 (http/protobuf default)",
    })
}

/// Initialize telemetry with the given configuration.
///
/// This will:
/// 1. Initialize OTLP trace export (if enabled)
/// 2. Initialize OTLP metrics export (if enabled)
/// 3. Initialize OTLP log export (if enabled) and console logging
///
/// Returns a `Telemetry` struct that must be kept alive for the duration of
/// the application and shut down on exit via [`Telemetry::shutdown`].
#[must_use]
pub fn init_telemetry(config: &TelemetryConfig) -> Telemetry {
    eprintln!("🦀 Sideways OTel: Initializing...");

    let _ = SERVICE_NAME.set(config.service_name.clone());

    let resource = resource::build_resource(config);

    let endpoint_description = describe_endpoint(config);

    let (tracer_provider, logger_provider) = if config.traces_enabled {
        match tracing::init_otlp_tracing(config, resource.clone()) {
            Ok((tp, lp)) => {
                eprintln!("✅ Sideways OTel: tracing initialized -> {endpoint_description}");
                if lp.is_some() {
                    eprintln!("✅ Sideways OTel: log export initialized");
                }
                (Some(tp), lp)
            }
            Err(err) => {
                eprintln!("⚠️  Sideways OTel: tracing unavailable: {err}");
                tracing::init_console_logging(config);
                (None, None)
            }
        }
    } else {
        eprintln!("📊 Sideways OTel: tracing disabled");
        tracing::init_console_logging(config);
        (None, None)
    };

    let meter_provider = if config.metrics_enabled {
        match metrics::init_otlp_metrics(config, resource) {
            Ok(mp) => {
                eprintln!("✅ Sideways OTel: metrics initialized -> {endpoint_description}");
                Some(mp)
            }
            Err(err) => {
                eprintln!("⚠️  Sideways OTel: metrics unavailable: {err}");
                None
            }
        }
    } else {
        eprintln!("📊 Sideways OTel: metrics disabled");
        None
    };

    Telemetry {
        tracer_provider,
        meter_provider,
        logger_provider,
    }
}
