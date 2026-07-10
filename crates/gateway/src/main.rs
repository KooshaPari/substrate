//! Substrate gateway — OpenAI-compatible HTTP surface for routing, A2A, and config.

use gateway::{serve, GatewayConfig};

/// Wire OTLP exporter (no-op when `OTEL_EXPORTER_OTLP_ENDPOINT` is unset).
fn init_telemetry() -> opentelemetry_sdk::trace::TracerProvider {
    use opentelemetry::trace::TracerProvider as _;
    use opentelemetry_sdk::trace::{Config, RandomIdGenerator, Sampler};
    use opentelemetry_sdk::Resource;
    use tracing_subscriber::prelude::*;

    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .build();

    let provider = opentelemetry_sdk::trace::TracerProvider::builder()
        .with_config(
            Config::default()
                .with_sampler(Sampler::TraceIdRatioBased(0.1))
                .with_id_generator(RandomIdGenerator::default()),
        )
        .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
        .with_resource(Resource::builder().with_service_name("substrate-gateway").build())
        .build();

    let tracer = provider.tracer("substrate-gateway");
    let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);

    tracing_subscriber::registry()
        .with(telemetry)
        .with(tracing_subscriber::fmt::layer().json())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    provider
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _provider = init_telemetry();

    let config = GatewayConfig::from_env()?;
    tracing::info!(
        bind = %config.bind,
        state_dir = %config.state_dir.display(),
        "gateway starting"
    );

    serve(config).await
}
