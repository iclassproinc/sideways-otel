use sideways_otel::{TelemetryConfig, init_telemetry_layer};

/// `force_flush` must be callable repeatedly (unlike `shutdown`) and must
/// never panic, including when every signal is disabled.
#[tokio::test(flavor = "multi_thread")]
async fn force_flush_is_repeatable_and_does_not_panic() {
    let config = TelemetryConfig::builder()
        .service_name("force-flush-test")
        .traces_enabled(false)
        .metrics_enabled(false)
        .logs_enabled(false)
        .build();
    let (telemetry, _layer) = init_telemetry_layer(&config);

    telemetry.force_flush();
    telemetry.force_flush();
    telemetry.shutdown();
}

mod panic_isolation {
    use opentelemetry::Context;
    use opentelemetry_sdk::{
        error::OTelSdkResult,
        trace::{SdkTracerProvider, Span, SpanData, SpanProcessor},
    };
    use sideways_otel::Telemetry;
    use std::time::Duration;

    #[derive(Debug)]
    struct PanickingProcessor;

    impl SpanProcessor for PanickingProcessor {
        fn on_start(&self, _span: &mut Span, _cx: &Context) {}
        fn on_end(&self, _span: SpanData) {}
        fn force_flush(&self) -> OTelSdkResult {
            panic!("boom");
        }
        fn shutdown_with_timeout(&self, _timeout: Duration) -> OTelSdkResult {
            Ok(())
        }
    }

    /// A panic inside a provider's flush must be reported, not re-raised on
    /// the calling thread (it would otherwise crash a Lambda handler).
    #[test]
    fn provider_panic_does_not_escape_force_flush() {
        let telemetry = Telemetry {
            tracer_provider: Some(
                SdkTracerProvider::builder()
                    .with_span_processor(PanickingProcessor)
                    .build(),
            ),
            meter_provider: None,
            logger_provider: None,
        };

        telemetry.force_flush();
    }
}
