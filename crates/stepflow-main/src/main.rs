use clap::Parser as _;
use stepflow_main::{Cli, Result};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt as _, util::SubscriberInitExt as _};

#[tokio::main]
async fn main() -> Result<()> {
    // Install stdio tracing logger.
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_timer(tracing_subscriber::fmt::time::LocalTime::rfc_3339());
    tracing_subscriber::registry()
        .with(EnvFilter::new("stepflow_=trace,info"))
        .with(fmt_layer)
        .with(tracing_error::ErrorLayer::default())
        .try_init()
        .unwrap();

    Cli::parse().execute().await?;
    Ok(())
}
