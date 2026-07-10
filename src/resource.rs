use crate::TelemetryConfig;
use opentelemetry::KeyValue;
use opentelemetry_sdk::Resource;

/// Build the OpenTelemetry `Resource` describing this service, from the
/// configured service name plus any extra resource attributes.
#[must_use] 
pub fn build_resource(config: &TelemetryConfig) -> Resource {
    let attributes: Vec<KeyValue> = config
        .resource_attributes
        .iter()
        .map(|(k, v)| KeyValue::new(k.clone(), v.clone()))
        .collect();

    Resource::builder()
        .with_service_name(config.service_name.clone())
        .with_attributes(attributes)
        .build()
}
