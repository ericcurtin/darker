//! `darker pull` command implementation

use crate::image::oci::ImageReference;
use crate::image::registry::RegistryClient;
use crate::storage::paths::DarkerPaths;
use clap::Args;

/// Arguments for the `pull` command
#[derive(Args)]
pub struct PullArgs {
    /// Image name to pull
    pub image: String,

    /// Download all tagged images in the repository
    #[arg(short, long)]
    pub all_tags: bool,

    /// Skip image verification
    #[arg(long)]
    pub disable_content_trust: bool,

    /// Set platform if server is multi-platform capable
    #[arg(long)]
    pub platform: Option<String>,

    /// Suppress verbose output
    #[arg(short, long)]
    pub quiet: bool,
}

/// Execute the `pull` command
pub async fn execute(args: PullArgs) -> anyhow::Result<()> {
    let paths = DarkerPaths::new()?;
    paths.ensure_directories()?;

    let image_ref = ImageReference::parse(&args.image)?;

    if !args.quiet {
        eprintln!(
            "Pulling from {}...",
            image_ref.repository_with_registry()
        );
    }

    let registry = RegistryClient::new()?;
    let image_id = registry.pull(&image_ref, &paths).await?;

    if !args.quiet {
        eprintln!("Digest: sha256:{}", &image_id[..12]);
        eprintln!(
            "Status: Downloaded newer image for {}",
            image_ref.full_name()
        );
    }

    println!("{}", image_ref.full_name());

    Ok(())
}
