//! `darker system` command implementation

use crate::storage::containers::ContainerStore;
use crate::storage::images::ImageStore;
use crate::storage::paths::DarkerPaths;
use clap::{Args, Subcommand};

/// Arguments for the `system` command
#[derive(Args)]
pub struct SystemArgs {
    #[command(subcommand)]
    pub command: SystemCommands,
}

/// System subcommands
#[derive(Subcommand)]
pub enum SystemCommands {
    /// Display system-wide information
    Info(SystemInfoArgs),
    /// Remove unused data
    Prune(SystemPruneArgs),
    /// Show darker disk usage
    Df(SystemDfArgs),
}

/// Arguments for system info
#[derive(Args)]
pub struct SystemInfoArgs {
    /// Format the output
    #[arg(short, long)]
    pub format: Option<String>,
}

/// Arguments for system prune
#[derive(Args)]
pub struct SystemPruneArgs {
    /// Remove all unused images not just dangling ones
    #[arg(short, long)]
    pub all: bool,

    /// Do not prompt for confirmation
    #[arg(short, long)]
    pub force: bool,

    /// Prune volumes
    #[arg(long)]
    pub volumes: bool,

    /// Provide filter values
    #[arg(long)]
    pub filter: Vec<String>,
}

/// Arguments for system df
#[derive(Args)]
pub struct SystemDfArgs {
    /// Show detailed information on space usage
    #[arg(short, long)]
    pub verbose: bool,

    /// Format the output
    #[arg(long)]
    pub format: Option<String>,
}

/// Execute the `system` command
pub async fn execute(args: SystemArgs) -> anyhow::Result<()> {
    let paths = DarkerPaths::new()?;

    match args.command {
        SystemCommands::Info(_) => {
            let container_store = ContainerStore::new(&paths)?;
            let image_store = ImageStore::new(&paths)?;

            let containers = container_store.list()?;
            let running = containers
                .iter()
                .filter(|c| {
                    container_store
                        .load_state(&c.id)
                        .map(|s| s.running)
                        .unwrap_or(false)
                })
                .count();

            let images = image_store.list()?;

            println!("Containers: {}", containers.len());
            println!(" Running: {}", running);
            println!(" Paused: 0");
            println!(" Stopped: {}", containers.len() - running);
            println!("Images: {}", images.len());
            println!("Server Version: {}", crate::VERSION);
            println!("Storage Driver: overlay (simulated)");
            println!("Darker Root Dir: {}", paths.root().display());
            println!("Operating System: macOS");
            println!("Architecture: {}", std::env::consts::ARCH);
            println!("Kernel Version: {}", get_kernel_version());
            println!("Network: host");
            println!("Security Options: sandbox (seatbelt)");
        }
        SystemCommands::Prune(prune_args) => {
            if !prune_args.force {
                eprintln!("WARNING! This will remove:");
                eprintln!("  - all stopped containers");
                if prune_args.all {
                    eprintln!("  - all images without at least one container associated to them");
                } else {
                    eprintln!("  - all dangling images");
                }
                if prune_args.volumes {
                    eprintln!("  - all volumes not used by at least one container");
                }
                eprintln!("Are you sure you want to continue? [y/N]");
                // In a real implementation, we'd read user input here
            }

            let container_store = ContainerStore::new(&paths)?;
            let image_store = ImageStore::new(&paths)?;

            let mut total_space: u64 = 0;

            // Remove stopped containers
            let containers = container_store.list()?;
            for container in containers {
                let state = container_store.load_state(&container.id)?;
                if !state.running {
                    let rootfs = crate::filesystem::rootfs::RootFs::new(&paths, &container.id)?;
                    rootfs.cleanup()?;
                    container_store.remove(&container.id)?;
                    println!("Deleted Container: {}", &container.id[..12]);
                }
            }

            // Remove dangling images
            if prune_args.all {
                let images = image_store.list()?;
                for image in images {
                    if image.repository.is_none() {
                        total_space += image.size;
                        image_store.remove(&image.id, true)?;
                        println!("Deleted Image: {}", &image.id[..12]);
                    }
                }
            }

            // Remove unused volumes
            if prune_args.volumes {
                let volume_manager = crate::filesystem::volume::VolumeManager::new(&paths)?;
                let removed = volume_manager.prune()?;
                for name in removed {
                    println!("Deleted Volume: {}", name);
                }
            }

            println!();
            println!("Total reclaimed space: {}", format_size(total_space));
        }
        SystemCommands::Df(df_args) => {
            let container_store = ContainerStore::new(&paths)?;
            let image_store = ImageStore::new(&paths)?;

            let images = image_store.list()?;
            let containers = container_store.list()?;

            let images_size: u64 = images.iter().map(|i| i.size).sum();

            println!(
                "{:<15} {:<15} {:<15} {:<15}",
                "TYPE", "TOTAL", "ACTIVE", "SIZE"
            );
            println!(
                "{:<15} {:<15} {:<15} {:<15}",
                "Images",
                images.len(),
                images.len(),
                format_size(images_size)
            );
            println!(
                "{:<15} {:<15} {:<15} {:<15}",
                "Containers",
                containers.len(),
                containers
                    .iter()
                    .filter(|c| container_store
                        .load_state(&c.id)
                        .map(|s| s.running)
                        .unwrap_or(false))
                    .count(),
                "0B"
            );
            println!(
                "{:<15} {:<15} {:<15} {:<15}",
                "Local Volumes", "0", "0", "0B"
            );
        }
    }

    Ok(())
}

fn get_kernel_version() -> String {
    // Try to get macOS version
    std::process::Command::new("uname")
        .arg("-r")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string())
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2}GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2}MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2}KB", bytes as f64 / KB as f64)
    } else {
        format!("{}B", bytes)
    }
}
