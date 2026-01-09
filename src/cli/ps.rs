//! `darker ps` command implementation

use crate::storage::containers::ContainerStore;
use crate::storage::paths::DarkerPaths;
use clap::Args;

/// Arguments for the `ps` command
#[derive(Args)]
pub struct PsArgs {
    /// Show all containers (default shows just running)
    #[arg(short, long)]
    pub all: bool,

    /// Filter output based on conditions provided
    #[arg(short, long)]
    pub filter: Vec<String>,

    /// Pretty-print containers using a Go template
    #[arg(long)]
    pub format: Option<String>,

    /// Show n last created containers (includes all states)
    #[arg(short = 'n', long)]
    pub last: Option<usize>,

    /// Show the latest created container (includes all states)
    #[arg(short, long)]
    pub latest: bool,

    /// Don't truncate output
    #[arg(long)]
    pub no_trunc: bool,

    /// Only display container IDs
    #[arg(short, long)]
    pub quiet: bool,

    /// Display total file sizes
    #[arg(short, long)]
    pub size: bool,
}

/// Execute the `ps` command
pub async fn execute(args: PsArgs) -> anyhow::Result<()> {
    let paths = DarkerPaths::new()?;
    let container_store = ContainerStore::new(&paths)?;

    let mut containers = container_store.list()?;

    // Filter running containers unless --all is specified
    if !args.all {
        containers.retain(|c| {
            container_store
                .load_state(&c.id)
                .map(|s| s.running)
                .unwrap_or(false)
        });
    }

    // Sort by creation time (newest first)
    containers.sort_by(|a, b| b.created.cmp(&a.created));

    // Apply --last or --latest filter
    if args.latest {
        containers.truncate(1);
    } else if let Some(n) = args.last {
        containers.truncate(n);
    }

    if containers.is_empty() {
        if !args.quiet {
            println!(
                "{:<15} {:<20} {:<20} {:<20} {:<15} {:<20} {:<20}",
                "CONTAINER ID", "IMAGE", "COMMAND", "CREATED", "STATUS", "PORTS", "NAMES"
            );
        }
        return Ok(());
    }

    if args.quiet {
        for container in containers {
            let id = if args.no_trunc {
                &container.id
            } else {
                &container.id[..12.min(container.id.len())]
            };
            println!("{}", id);
        }
        return Ok(());
    }

    // Print header
    println!(
        "{:<15} {:<20} {:<20} {:<20} {:<15} {:<10} {:<20}",
        "CONTAINER ID", "IMAGE", "COMMAND", "CREATED", "STATUS", "PORTS", "NAMES"
    );

    for container in containers {
        let state = container_store
            .load_state(&container.id)
            .unwrap_or_default();

        let id = if args.no_trunc {
            container.id.clone()
        } else {
            container.id[..12.min(container.id.len())].to_string()
        };

        let command = container.command.join(" ");
        let command = if args.no_trunc {
            command
        } else if command.len() > 20 {
            format!("{}...", &command[..17])
        } else {
            command
        };

        let created = format_time_ago(container.created);
        let status = format_status(&state);
        let ports = ""; // Host networking doesn't expose ports

        println!(
            "{:<15} {:<20} {:<20} {:<20} {:<15} {:<10} {:<20}",
            id, container.image, command, created, status, ports, container.name
        );
    }

    Ok(())
}

/// Format container status
fn format_status(state: &crate::storage::containers::ContainerState) -> String {
    if state.running {
        if state.paused {
            "Paused".to_string()
        } else {
            let uptime = chrono::Utc::now().signed_duration_since(state.started_at);
            format!("Up {}", format_duration(uptime))
        }
    } else if state.exit_code.is_some() {
        let exit_code = state.exit_code.unwrap_or(0);
        let finished = state
            .finished_at
            .map(|t| format_time_ago(t))
            .unwrap_or_else(|| "unknown".to_string());
        format!("Exited ({}) {}", exit_code, finished)
    } else {
        "Created".to_string()
    }
}

/// Format a duration as a human-readable string
fn format_duration(duration: chrono::Duration) -> String {
    if duration.num_days() > 0 {
        format!("{} days", duration.num_days())
    } else if duration.num_hours() > 0 {
        format!("{} hours", duration.num_hours())
    } else if duration.num_minutes() > 0 {
        format!("{} minutes", duration.num_minutes())
    } else {
        format!("{} seconds", duration.num_seconds())
    }
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
