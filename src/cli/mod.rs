//! CLI command definitions and handlers

pub mod build;
pub mod exec;
pub mod images;
pub mod inspect;
pub mod logs;
pub mod network;
pub mod ps;
pub mod pull;
pub mod push;
pub mod rm;
pub mod run;
pub mod start;
pub mod stop;
pub mod system;
pub mod tag;
pub mod volume;

use clap::{Parser, Subcommand};

/// Darker - A Docker-like container runtime for macOS
#[derive(Parser)]
#[command(name = "darker")]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

/// Available commands
#[derive(Subcommand)]
pub enum Commands {
    /// Create and run a container
    Run(run::RunArgs),

    /// Execute a command in a running container
    Exec(exec::ExecArgs),

    /// Build an image from a Dockerfile
    Build(build::BuildArgs),

    /// List images
    Images(images::ImagesArgs),

    /// List containers
    Ps(ps::PsArgs),

    /// Remove one or more containers
    Rm(rm::RmArgs),

    /// Remove one or more images
    Rmi(rm::RmiArgs),

    /// Pull an image from a registry
    Pull(pull::PullArgs),

    /// Push an image to a registry
    Push(push::PushArgs),

    /// Fetch the logs of a container
    Logs(logs::LogsArgs),

    /// Start one or more stopped containers
    Start(start::StartArgs),

    /// Stop one or more running containers
    Stop(stop::StopArgs),

    /// Restart one or more containers
    Restart(start::RestartArgs),

    /// Return low-level information on containers or images
    Inspect(inspect::InspectArgs),

    /// Create a tag for an image
    Tag(tag::TagArgs),

    /// Manage volumes
    Volume(volume::VolumeArgs),

    /// Manage networks
    Network(network::NetworkArgs),

    /// Manage Darker
    System(system::SystemArgs),

    /// Attach to a running container
    Attach(exec::AttachArgs),
}
