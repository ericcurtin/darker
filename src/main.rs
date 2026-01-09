//! Darker CLI entry point
//!
//! A Docker-like container runtime for macOS using native Darwin APIs.

use clap::Parser;
use darker::cli::{Cli, Commands};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Run(args) => darker::cli::run::execute(args).await,
        Commands::Exec(args) => darker::cli::exec::execute(args).await,
        Commands::Build(args) => darker::cli::build::execute(args).await,
        Commands::Images(args) => darker::cli::images::execute(args).await,
        Commands::Ps(args) => darker::cli::ps::execute(args).await,
        Commands::Rm(args) => darker::cli::rm::execute(args).await,
        Commands::Rmi(args) => darker::cli::rm::execute_rmi(args).await,
        Commands::Pull(args) => darker::cli::pull::execute(args).await,
        Commands::Push(args) => darker::cli::push::execute(args).await,
        Commands::Logs(args) => darker::cli::logs::execute(args).await,
        Commands::Start(args) => darker::cli::start::execute(args).await,
        Commands::Stop(args) => darker::cli::stop::execute(args).await,
        Commands::Restart(args) => darker::cli::start::execute_restart(args).await,
        Commands::Inspect(args) => darker::cli::inspect::execute(args).await,
        Commands::Tag(args) => darker::cli::tag::execute(args).await,
        Commands::Volume(args) => darker::cli::volume::execute(args).await,
        Commands::Network(args) => darker::cli::network::execute(args).await,
        Commands::System(args) => darker::cli::system::execute(args).await,
        Commands::Attach(args) => darker::cli::exec::execute_attach(args).await,
    }
}
