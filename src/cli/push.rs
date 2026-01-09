//! `darker push` command implementation

use crate::image::oci::ImageReference;
use crate::image::registry::RegistryClient;
use crate::storage::images::ImageStore;
use crate::storage::paths::DarkerPaths;
use crate::DarkerError;
use clap::Args;

/// Arguments for the `push` command
#[derive(Args)]
pub struct PushArgs {
    /// Image name to push
    pub image: String,

    /// Push all tagged images in the repository
    #[arg(short, long)]
    pub all_tags: bool,

    /// Skip image signing
    #[arg(long)]
    pub disable_content_trust: bool,

    /// Suppress verbose output
    #[arg(short, long)]
    pub quiet: bool,
}

/// Execute the `push` command
pub async fn execute(args: PushArgs) -> anyhow::Result<()> {
    let paths = DarkerPaths::new()?;
    let image_store = ImageStore::new(&paths)?;

    let image_ref = ImageReference::parse(&args.image)?;

    // Find the image locally
    let image_id = image_store
        .find_image(&image_ref)
        .ok_or_else(|| DarkerError::ImageNotFound(args.image.clone()))?;

    if !args.quiet {
        eprintln!("The push refers to repository [{}]", image_ref.repository_with_registry());
    }

    let registry = RegistryClient::new()?;
    registry.push(&image_ref, &image_id, &paths).await?;

    if !args.quiet {
        eprintln!(
            "{}: digest: sha256:{} size: unknown",
            image_ref.tag(),
            &image_id[..12]
        );
    }

    Ok(())
}
