//! `darker tag` command implementation

use crate::image::oci::ImageReference;
use crate::storage::images::ImageStore;
use crate::storage::paths::DarkerPaths;
use crate::DarkerError;
use clap::Args;

/// Arguments for the `tag` command
#[derive(Args)]
pub struct TagArgs {
    /// Source image
    pub source_image: String,

    /// Target image
    pub target_image: String,
}

/// Execute the `tag` command
pub async fn execute(args: TagArgs) -> anyhow::Result<()> {
    let paths = DarkerPaths::new()?;
    let image_store = ImageStore::new(&paths)?;

    let source_ref = ImageReference::parse(&args.source_image)?;
    let target_ref = ImageReference::parse(&args.target_image)?;

    // Find source image
    let image_id = image_store
        .find_image(&source_ref)
        .ok_or_else(|| DarkerError::ImageNotFound(args.source_image.clone()))?;

    // Create new tag
    image_store.tag(&image_id, &target_ref)?;

    Ok(())
}
