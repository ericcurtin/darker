//! `darker start` and `darker restart` command implementations

use crate::runtime::container::Container;
use crate::storage::containers::ContainerStore;
use crate::storage::paths::DarkerPaths;
use clap::Args;

/// Arguments for the `start` command
#[derive(Args)]
pub struct StartArgs {
    /// Container names or IDs to start
    pub containers: Vec<String>,

    /// Attach STDOUT/STDERR and forward signals
    #[arg(short, long)]
    pub attach: bool,

    /// Attach to STDIN
    #[arg(short, long)]
    pub interactive: bool,

    /// Override the key sequence for detaching a container
    #[arg(long)]
    pub detach_keys: Option<String>,
}

/// Arguments for the `restart` command
#[derive(Args)]
pub struct RestartArgs {
    /// Container names or IDs to restart
    pub containers: Vec<String>,

    /// Seconds to wait for stop before killing the container
    #[arg(short, long, default_value = "10")]
    pub time: u64,
}

/// Execute the `start` command
pub async fn execute(args: StartArgs) -> anyhow::Result<()> {
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
        if state.running {
            eprintln!("Container {} is already running", container_ref);
            continue;
        }

        let config = container_store.load(&container_id)?;
        let mut container = Container::from_config(config, &paths)?;

        if args.attach {
            let exit_code = container.run(false, args.interactive).await?;
            std::process::exit(exit_code);
        } else {
            container.start_detached().await?;
            println!("{}", container_id);
        }
    }

    Ok(())
}

/// Execute the `restart` command
pub async fn execute_restart(args: RestartArgs) -> anyhow::Result<()> {
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

        let config = container_store.load(&container_id)?;
        let container = Container::from_config(config.clone(), &paths)?;

        // Stop if running
        let state = container_store.load_state(&container_id)?;
        if state.running {
            container.stop(Some(args.time)).await?;
        }

        // Start again
        let mut container = Container::from_config(config, &paths)?;
        container.start_detached().await?;

        println!("{}", container_id);
    }

    Ok(())
}
