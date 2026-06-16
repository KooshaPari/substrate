//! Substrate HTTP driver — REST API for dispatch, plan, route, and mailbox.

use driver_http::{serve, HttpConfig};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = HttpConfig::from_env()?;
    eprintln!(
        "substrate-http listening on {} (state: {})",
        config.bind,
        config.state_dir.display()
    );
    serve(config).await
}
