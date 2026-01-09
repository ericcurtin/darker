//! `darker build` command implementation

use crate::image::build::ImageBuilder;
use crate::storage::paths::DarkerPaths;
use clap::Args;
use std::path::PathBuf;

/// Valid container file names in order of preference
pub const CONTAINER_FILE_NAMES: &[&str] = &["Darkerfile", "Dockerfile", "Containerfile"];

/// Arguments for the `build` command
#[derive(Args)]
pub struct BuildArgs {
    /// Path to build context
    #[arg(default_value = ".")]
    pub path: PathBuf,

    /// Name and optionally a tag in the 'name:tag' format
    #[arg(short, long)]
    pub tag: Option<String>,

    /// Name of the Dockerfile (auto-detects Darkerfile, Dockerfile, or Containerfile)
    #[arg(short, long)]
    pub file: Option<String>,

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

/// Find the container file in the build context
fn find_container_file(context_path: &PathBuf, explicit_file: Option<&str>) -> anyhow::Result<String> {
    // If explicitly specified, use that
    if let Some(file) = explicit_file {
        let path = context_path.join(file);
        if path.exists() {
            return Ok(file.to_string());
        }
        anyhow::bail!("Cannot find {} at {}", file, path.display());
    }

    // Auto-detect: try Darkerfile, Dockerfile, Containerfile in order
    for name in CONTAINER_FILE_NAMES {
        let path = context_path.join(name);
        if path.exists() {
            return Ok(name.to_string());
        }
    }

    anyhow::bail!(
        "Cannot find container file. Looked for: {}",
        CONTAINER_FILE_NAMES.join(", ")
    )
}

/// Execute the `build` command
pub async fn execute(args: BuildArgs) -> anyhow::Result<()> {
    let paths = DarkerPaths::new()?;
    paths.ensure_directories()?;

    let container_file = find_container_file(&args.path, args.file.as_deref())?;

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
        eprintln!("Using {} as container file", container_file);
        eprintln!("Sending build context to Darker...");
    }

    let image_id = builder
        .build(
            &args.path,
            &container_file,
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
