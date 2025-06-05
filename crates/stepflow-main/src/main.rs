use clap::Parser as _;
use stepflow_main::{Cli, Result, cli::init_tracing};

async fn run(cli: Cli) -> Result<()> {
    // Initialize tracing with the specified configuration
    init_tracing(
        &cli.log_level,
        &cli.other_log_level,
        cli.log_file.as_deref(),
    )?;

    cli.execute().await
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let omit_stack_trace = cli.omit_stack_trace;

    if let Err(e) = run(cli).await {
        #[allow(clippy::print_stderr)]
        if !omit_stack_trace {
            eprintln!("{:?}", e);
        } else {
            eprintln!("{}", e);
        }
        std::process::exit(1);
    }
}
