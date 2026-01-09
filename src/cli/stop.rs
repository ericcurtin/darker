//! `darker stop` command implementation

use crate::runtime::container::Container;
use crate::storage::containers::ContainerStore;
use crate::storage::paths::DarkerPaths;
use crate::DarkerError;
use clap::Args;

/// Arguments for the `stop` command
#[derive(Args)]
pub struct StopArgs {
    /// Container names or IDs to stop
    pub containers: Vec<String>,

    /// Seconds to wait for stop before killing it
    #[arg(short, long, default_value = "10")]
    pub time: u64,
}

/// Execute the `stop` command
pub async fn execute(args: StopArgs) -> anyhow::Result<()> {
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

        let state = container_store.load_state(&container_id)?;
        if !state.running {
            eprintln!("Container {} is not running", container_ref);
            continue;
        }

        let config = container_store.load(&container_id)?;
        let container = Container::from_config(config, &paths)?;
        container.stop(Some(args.time)).await?;

        println!("{}", container_id);
    }

    Ok(())
}
