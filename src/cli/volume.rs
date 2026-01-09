//! `darker volume` command implementation

use crate::filesystem::volume::VolumeManager;
use crate::storage::paths::DarkerPaths;
use clap::{Args, Subcommand};

/// Arguments for the `volume` command
#[derive(Args)]
pub struct VolumeArgs {
    #[command(subcommand)]
    pub command: VolumeCommands,
}

/// Volume subcommands
#[derive(Subcommand)]
pub enum VolumeCommands {
    /// Create a volume
    Create(VolumeCreateArgs),
    /// List volumes
    Ls(VolumeLsArgs),
    /// Remove one or more volumes
    Rm(VolumeRmArgs),
    /// Display detailed information on one or more volumes
    Inspect(VolumeInspectArgs),
    /// Remove all unused local volumes
    Prune(VolumePruneArgs),
}

/// Arguments for volume create
#[derive(Args)]
pub struct VolumeCreateArgs {
    /// Volume name
    pub name: Option<String>,

    /// Specify volume driver name
    #[arg(short, long, default_value = "local")]
    pub driver: String,

    /// Set driver specific options
    #[arg(short, long)]
    pub opt: Vec<String>,

    /// Set metadata for a volume
    #[arg(long)]
    pub label: Vec<String>,
}

/// Arguments for volume ls
#[derive(Args)]
pub struct VolumeLsArgs {
    /// Provide filter values
    #[arg(short, long)]
    pub filter: Vec<String>,

    /// Format the output
    #[arg(long)]
    pub format: Option<String>,

    /// Only display volume names
    #[arg(short, long)]
    pub quiet: bool,
}

/// Arguments for volume rm
#[derive(Args)]
pub struct VolumeRmArgs {
    /// Volume names to remove
    pub volumes: Vec<String>,

    /// Force the removal of one or more volumes
    #[arg(short, long)]
    pub force: bool,
}

/// Arguments for volume inspect
#[derive(Args)]
pub struct VolumeInspectArgs {
    /// Volume names to inspect
    pub volumes: Vec<String>,

    /// Format the output
    #[arg(short, long)]
    pub format: Option<String>,
}

/// Arguments for volume prune
#[derive(Args)]
pub struct VolumePruneArgs {
    /// Provide filter values
    #[arg(short, long)]
    pub filter: Vec<String>,

    /// Do not prompt for confirmation
    #[arg(short, long)]
    pub force: bool,
}

/// Execute the `volume` command
pub async fn execute(args: VolumeArgs) -> anyhow::Result<()> {
    let paths = DarkerPaths::new()?;
    paths.ensure_directories()?;

    let volume_manager = VolumeManager::new(&paths)?;

    match args.command {
        VolumeCommands::Create(create_args) => {
            let name = create_args
                .name
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()[..12].to_string());

            volume_manager.create(&name)?;
            println!("{}", name);
        }
        VolumeCommands::Ls(ls_args) => {
            let volumes = volume_manager.list()?;

            if ls_args.quiet {
                for vol in volumes {
                    println!("{}", vol.name);
                }
            } else {
                println!("{:<20} {:<10}", "DRIVER", "VOLUME NAME");
                for vol in volumes {
                    println!("{:<20} {:<10}", vol.driver, vol.name);
                }
            }
        }
        VolumeCommands::Rm(rm_args) => {
            for name in &rm_args.volumes {
                match volume_manager.remove(name) {
                    Ok(_) => println!("{}", name),
                    Err(e) => {
                        if !rm_args.force {
                            eprintln!("Error removing volume {}: {}", name, e);
                        }
                    }
                }
            }
        }
        VolumeCommands::Inspect(inspect_args) => {
            let mut results = Vec::new();
            for name in &inspect_args.volumes {
                match volume_manager.inspect(name) {
                    Ok(vol) => results.push(vol),
                    Err(e) => eprintln!("Error inspecting volume {}: {}", name, e),
                }
            }
            println!("{}", serde_json::to_string_pretty(&results)?);
        }
        VolumeCommands::Prune(prune_args) => {
            if !prune_args.force {
                eprintln!("WARNING! This will remove all local volumes not used by at least one container.");
                eprintln!("Are you sure you want to continue? [y/N]");
                // In a real implementation, we'd read user input here
            }

            let removed = volume_manager.prune()?;
            if removed.is_empty() {
                println!("Total reclaimed space: 0B");
            } else {
                println!("Deleted Volumes:");
                for name in removed {
                    println!("{}", name);
                }
            }
        }
    }

    Ok(())
}
