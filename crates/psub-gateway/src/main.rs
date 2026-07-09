//! Substrate gateway — OpenAI-compatible HTTP surface for routing, A2A, and config.

use psub_gateway::{serve, GatewayConfig};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Initialize structured logging and optional OpenTelemetry OTLP export.
///
/// Honors `RUST_LOG` for the text layer. OpenTelemetry is enabled when
/// `OTEL_EXPORTER_OTLP_ENDPOINT` is set (e.g.
/// `http://localhost:4318`).  Traces are shipped to the configured
/// OTLP endpoint via the `tracing-opentelemetry` bridge.
fn init_telemetry() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,psub_gateway=debug,psub_gateway::dispatch=trace"));

    let fmt_layer = fmt::layer()
        .with_target(true)
        .with_thread_ids(false)
        .with_line_number(true);

    // Only wire the OTLP layer when the endpoint env var is set.
    // This keeps the dependency tree clean for offline / dev builds.
    let otel_endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok();
    if let Some(endpoint) = otel_endpoint {
        let resource = opentelemetry_sdk::Resource::builder()
            .with_attribute(opentelemetry::KeyValue::new(
                opentelemetry_sdk::Resource::SERVICE_NAME,
                "substrate-gateway",
            ))
            .build();

        let tracer = opentelemetry_otlp::new_pipeline()
            .tracing()
            .with_exporter(
                opentelemetry_otlp::new_exporter()
                    .tonic()
                    .with_endpoint(&endpoint),
            )
            .with_trace_config(
                opentelemetry_sdk::trace::Config::default()
                    .with_resource(resource)
                    .with_sampler(opentelemetry_sdk::trace::Sampler::ParentBased(
                        Box::new(opentelemetry_sdk::trace::Sampler::TraceIdRatio(0.1)),
                    )),
            )
            .install_batch(opentelemetry_sdk::runtime::Tokio)
            .expect("OTLP tracer pipeline must install");

        let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

        tracing_subscriber::registry()
            .with(filter)
            .with(fmt_layer)
            .with(otel_layer)
            .init();
    } else {
        tracing_subscriber::registry()
            .with(filter)
            .with(fmt_layer)
            .init();
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();
    let config = GatewayConfig::from_env()?;
    tracing::info!(
        bind = %config.bind,
        state_dir = %config.state_dir.display(),
        "substrate-gateway starting"
    );
    serve(config).await
}
