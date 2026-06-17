//! Substrate gateway — OpenAI-compatible HTTP surface for routing, A2A, and config.

use gateway::{serve, GatewayConfig};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = GatewayConfig::from_env()?;
    eprintln!(
        "substrate-gateway listening on {} (state: {})",
        config.bind,
        config.state_dir.display()
    );
    serve(config).await
}
