//! `darker rm` and `darker rmi` command implementations

use crate::storage::containers::ContainerStore;
use crate::storage::images::ImageStore;
use crate::storage::paths::DarkerPaths;
use crate::{DarkerError, Result};
use clap::Args;

/// Arguments for the `rm` command
#[derive(Args)]
pub struct RmArgs {
    /// Container names or IDs to remove
    pub containers: Vec<String>,

    /// Force the removal of a running container
    #[arg(short, long)]
    pub force: bool,

    /// Remove anonymous volumes associated with the container
    #[arg(short, long)]
    pub volumes: bool,

    /// Remove the specified link
    #[arg(short, long)]
    pub link: bool,
}

/// Arguments for the `rmi` command
#[derive(Args)]
pub struct RmiArgs {
    /// Image names or IDs to remove
    pub images: Vec<String>,

    /// Force removal of the image
    #[arg(short, long)]
    pub force: bool,

    /// Do not delete untagged parent images
    #[arg(long)]
    pub no_prune: bool,
}

/// Execute the `rm` command
pub async fn execute(args: RmArgs) -> anyhow::Result<()> {
    let paths = DarkerPaths::new()?;
    let container_store = ContainerStore::new(&paths)?;

    for container_ref in &args.containers {
        let container_id = match container_store.find(container_ref) {
            Some(id) => id,
            None => {
                eprintln!("Error: No such container: {}", container_ref);
                continue;
            }
        };

        // Check if container is running
        let state = container_store.load_state(&container_id)?;
        if state.running && !args.force {
            eprintln!(
                "Error: Container {} is running. Stop it first or use --force",
                container_ref
            );
            continue;
        }

        // Stop if running and force is set
        if state.running && args.force {
            let config = container_store.load(&container_id)?;
            let container = crate::runtime::container::Container::from_config(config, &paths)?;
            container.stop(None).await?;
        }

        // Remove container rootfs
        let rootfs = crate::filesystem::rootfs::RootFs::new(&paths, &container_id)?;
        rootfs.cleanup()?;

        // Remove container metadata
        container_store.remove(&container_id)?;

        println!("{}", container_id);
    }

    Ok(())
}

/// Execute the `rmi` command
pub async fn execute_rmi(args: RmiArgs) -> anyhow::Result<()> {
    let paths = DarkerPaths::new()?;
    let image_store = ImageStore::new(&paths)?;
    let container_store = ContainerStore::new(&paths)?;

    for image_ref in &args.images {
        let image_id = match image_store.find(image_ref) {
            Some(id) => id,
            None => {
                eprintln!("Error: No such image: {}", image_ref);
                continue;
            }
        };

        // Check if any containers are using this image
        let containers = container_store.list()?;
        let using_containers: Vec<_> = containers
            .iter()
            .filter(|c| c.image_id == image_id)
            .collect();

        if !using_containers.is_empty() && !args.force {
            eprintln!(
                "Error: Cannot remove image {}: used by {} container(s)",
                image_ref,
                using_containers.len()
            );
            continue;
        }

        // Remove image
        image_store.remove(&image_id, !args.no_prune)?;

        println!("Deleted: {}", image_id);
    }

    Ok(())
}
