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
