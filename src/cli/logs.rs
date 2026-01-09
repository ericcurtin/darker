//! `darker logs` command implementation

use crate::storage::containers::ContainerStore;
use crate::storage::paths::DarkerPaths;
use crate::DarkerError;
use clap::Args;
use std::io::{BufRead, BufReader};

/// Arguments for the `logs` command
#[derive(Args)]
pub struct LogsArgs {
    /// Container name or ID
    pub container: String,

    /// Show extra details provided to logs
    #[arg(long)]
    pub details: bool,

    /// Follow log output
    #[arg(short, long)]
    pub follow: bool,

    /// Show logs since timestamp (e.g., 2013-01-02T13:23:37Z) or relative (e.g., 42m)
    #[arg(long)]
    pub since: Option<String>,

    /// Number of lines to show from the end of the logs
    #[arg(short = 'n', long, default_value = "all")]
    pub tail: String,

    /// Show timestamps
    #[arg(short, long)]
    pub timestamps: bool,

    /// Show logs before a timestamp or relative time
    #[arg(long)]
    pub until: Option<String>,
}

/// Execute the `logs` command
pub async fn execute(args: LogsArgs) -> anyhow::Result<()> {
    let paths = DarkerPaths::new()?;
    let container_store = ContainerStore::new(&paths)?;

    // Find container
    let container_id = container_store
        .find(&args.container)
        .ok_or_else(|| DarkerError::ContainerNotFound(args.container.clone()))?;

    let log_path = paths.container_log(&container_id);

    if !log_path.exists() {
        return Ok(()); // No logs yet
    }

    let file = std::fs::File::open(&log_path)?;
    let reader = BufReader::new(file);
    let lines: Vec<String> = reader.lines().filter_map(|l| l.ok()).collect();

    // Handle tail parameter
    let tail_count = if args.tail == "all" {
        lines.len()
    } else {
        args.tail.parse().unwrap_or(lines.len())
    };

    let start_idx = if lines.len() > tail_count {
        lines.len() - tail_count
    } else {
        0
    };

    for line in &lines[start_idx..] {
        println!("{}", line);
    }

    // If follow mode, watch for new lines
    if args.follow {
        use std::io::Seek;
        use tokio::time::{sleep, Duration};

        let mut file = std::fs::File::open(&log_path)?;
        file.seek(std::io::SeekFrom::End(0))?;
        let mut reader = BufReader::new(file);

        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => {
                    // No new data, wait a bit
                    sleep(Duration::from_millis(100)).await;
                }
                Ok(_) => {
                    print!("{}", line);
                }
                Err(e) => {
                    eprintln!("Error reading logs: {}", e);
                    break;
                }
            }
        }
    }

    Ok(())
}
