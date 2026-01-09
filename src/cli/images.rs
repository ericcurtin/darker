//! `darker images` command implementation

use crate::storage::images::ImageStore;
use crate::storage::paths::DarkerPaths;
use clap::Args;

/// Arguments for the `images` command
#[derive(Args)]
pub struct ImagesArgs {
    /// Repository name to filter by
    pub repository: Option<String>,

    /// Show all images (default hides intermediate images)
    #[arg(short, long)]
    pub all: bool,

    /// Show digests
    #[arg(long)]
    pub digests: bool,

    /// Only show image IDs
    #[arg(short, long)]
    pub quiet: bool,

    /// Don't truncate output
    #[arg(long)]
    pub no_trunc: bool,

    /// Filter output based on conditions provided
    #[arg(short, long)]
    pub filter: Vec<String>,

    /// Pretty-print images using a Go template
    #[arg(long)]
    pub format: Option<String>,
}

/// Execute the `images` command
pub async fn execute(args: ImagesArgs) -> anyhow::Result<()> {
    let paths = DarkerPaths::new()?;
    let image_store = ImageStore::new(&paths)?;

    let images = image_store.list()?;

    if images.is_empty() {
        if !args.quiet {
            println!("REPOSITORY          TAG                 IMAGE ID            CREATED             SIZE");
        }
        return Ok(());
    }

    // Filter by repository if specified
    let filtered: Vec<_> = if let Some(ref repo) = args.repository {
        images
            .into_iter()
            .filter(|img| img.repository.as_deref() == Some(repo.as_str()))
            .collect()
    } else {
        images
    };

    if args.quiet {
        for image in filtered {
            let id = if args.no_trunc {
                &image.id
            } else {
                &image.id[..12.min(image.id.len())]
            };
            println!("{}", id);
        }
        return Ok(());
    }

    // Print header
    if args.digests {
        println!(
            "{:<20} {:<20} {:<72} {:<20} {:<20} {:<10}",
            "REPOSITORY", "TAG", "DIGEST", "IMAGE ID", "CREATED", "SIZE"
        );
    } else {
        println!(
            "{:<20} {:<20} {:<20} {:<20} {:<10}",
            "REPOSITORY", "TAG", "IMAGE ID", "CREATED", "SIZE"
        );
    }

    for image in filtered {
        let repo = image.repository.as_deref().unwrap_or("<none>");
        let tag = image.tag.as_deref().unwrap_or("<none>");
        let id = if args.no_trunc {
            image.id.clone()
        } else {
            image.id[..12.min(image.id.len())].to_string()
        };
        let created = format_time_ago(image.created);
        let size = format_size(image.size);

        if args.digests {
            let digest = image.digest.as_deref().unwrap_or("<none>");
            println!(
                "{:<20} {:<20} {:<72} {:<20} {:<20} {:<10}",
                repo, tag, digest, id, created, size
            );
        } else {
            println!(
                "{:<20} {:<20} {:<20} {:<20} {:<10}",
                repo, tag, id, created, size
            );
        }
    }

    Ok(())
}

/// Format a timestamp as a human-readable "time ago" string
fn format_time_ago(time: chrono::DateTime<chrono::Utc>) -> String {
    let now = chrono::Utc::now();
    let duration = now.signed_duration_since(time);

    if duration.num_days() > 365 {
        format!("{} years ago", duration.num_days() / 365)
    } else if duration.num_days() > 30 {
        format!("{} months ago", duration.num_days() / 30)
    } else if duration.num_days() > 7 {
        format!("{} weeks ago", duration.num_days() / 7)
    } else if duration.num_days() > 0 {
        format!("{} days ago", duration.num_days())
    } else if duration.num_hours() > 0 {
        format!("{} hours ago", duration.num_hours())
    } else if duration.num_minutes() > 0 {
        format!("{} minutes ago", duration.num_minutes())
    } else {
        "Less than a minute ago".to_string()
    }
}

/// Format a size in bytes as a human-readable string
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
