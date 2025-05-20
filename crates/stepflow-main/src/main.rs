use clap::Parser as _;
use stepflow_main::{Cli, Result};

#[tokio::main]
async fn main() -> Result<()> {
    // Install stdio tracing logger.
    tracing_subscriber::FmtSubscriber::builder()
        .with_timer(tracing_subscriber::fmt::time::LocalTime::rfc_3339())
        .init();

    Cli::parse().execute().await?;
    Ok(())
}
