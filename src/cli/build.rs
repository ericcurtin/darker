//! `darker build` command implementation

use crate::image::build::ImageBuilder;
use crate::storage::paths::DarkerPaths;
use clap::Args;
use std::path::PathBuf;

/// Arguments for the `build` command
#[derive(Args)]
pub struct BuildArgs {
    /// Path to build context
    #[arg(default_value = ".")]
    pub path: PathBuf,

    /// Name and optionally a tag in the 'name:tag' format
    #[arg(short, long)]
    pub tag: Option<String>,

    /// Name of the Dockerfile
    #[arg(short, long, default_value = "Dockerfile")]
    pub file: String,

    /// Set build-time variables
    #[arg(long)]
    pub build_arg: Vec<String>,

    /// Do not use cache when building the image
    #[arg(long)]
    pub no_cache: bool,

    /// Always attempt to pull a newer version of the image
    #[arg(long)]
    pub pull: bool,

    /// Suppress the build output and print image ID on success
    #[arg(short, long)]
    pub quiet: bool,

    /// Remove intermediate containers after a successful build
    #[arg(long, default_value = "true")]
    pub rm: bool,

    /// Set the target build stage to build
    #[arg(long)]
    pub target: Option<String>,

    /// Set the networking mode for the RUN instructions during build
    #[arg(long, default_value = "host")]
    pub network: String,

    /// Set platform if the Dockerfile uses FROM --platform
    #[arg(long)]
    pub platform: Option<String>,
}

/// Execute the `build` command
pub async fn execute(args: BuildArgs) -> anyhow::Result<()> {
    let paths = DarkerPaths::new()?;
    paths.ensure_directories()?;

    let dockerfile_path = args.path.join(&args.file);
    if !dockerfile_path.exists() {
        anyhow::bail!("Cannot find Dockerfile at {}", dockerfile_path.display());
    }

    // Parse build args
    let build_args: std::collections::HashMap<String, String> = args
        .build_arg
        .iter()
        .filter_map(|arg| {
            let parts: Vec<&str> = arg.splitn(2, '=').collect();
            if parts.len() == 2 {
                Some((parts[0].to_string(), parts[1].to_string()))
            } else {
                None
            }
        })
        .collect();

    let mut builder = ImageBuilder::new(&paths)?;

    if !args.quiet {
        eprintln!("Sending build context to Darker...");
    }

    let image_id = builder
        .build(
            &args.path,
            &args.file,
            args.tag.as_deref(),
            &build_args,
            args.no_cache,
            args.target.as_deref(),
            !args.quiet,
        )
        .await?;

    if args.quiet {
        println!("{}", image_id);
    } else {
        eprintln!("Successfully built {}", &image_id[..12]);
        if let Some(tag) = args.tag {
            eprintln!("Successfully tagged {}", tag);
        }
    }

    Ok(())
}
