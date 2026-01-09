//! `darker exec` command implementation

use crate::runtime::container::Container;
use crate::storage::containers::ContainerStore;
use crate::storage::paths::DarkerPaths;
use crate::DarkerError;
use clap::Args;

/// Arguments for the `exec` command
#[derive(Args)]
pub struct ExecArgs {
    /// Container name or ID
    pub container: String,

    /// Command to execute
    pub command: Vec<String>,

    /// Run command in detached mode
    #[arg(short, long)]
    pub detach: bool,

    /// Set environment variables
    #[arg(short, long)]
    pub env: Vec<String>,

    /// Keep STDIN open
    #[arg(short, long)]
    pub interactive: bool,

    /// Allocate a pseudo-TTY
    #[arg(short, long)]
    pub tty: bool,

    /// Username or UID
    #[arg(short, long)]
    pub user: Option<String>,

    /// Working directory inside the container
    #[arg(short, long)]
    pub workdir: Option<String>,
}

/// Arguments for the `attach` command
#[derive(Args)]
pub struct AttachArgs {
    /// Container name or ID
    pub container: String,

    /// Do not attach STDIN
    #[arg(long)]
    pub no_stdin: bool,

    /// Proxy all received signals to the process
    #[arg(long)]
    pub sig_proxy: bool,
}

/// Execute the `exec` command
pub async fn execute(args: ExecArgs) -> anyhow::Result<()> {
    let paths = DarkerPaths::new()?;
    let container_store = ContainerStore::new(&paths)?;

    // Find container
    let container_id = container_store
        .find(&args.container)
        .ok_or_else(|| DarkerError::ContainerNotFound(args.container.clone()))?;

    let config = container_store.load(&container_id)?;

    // Check if container is running
    let state = container_store.load_state(&container_id)?;
    if !state.running {
        return Err(DarkerError::ContainerNotRunning(args.container).into());
    }

    // Build command
    let cmd = if args.command.is_empty() {
        vec!["/bin/sh".to_string()]
    } else {
        args.command
    };

    // Execute command in container
    let container = Container::from_config(config, &paths)?;
    let exit_code = container
        .exec(
            &cmd,
            &args.env,
            args.workdir.as_deref(),
            args.user.as_deref(),
            args.tty,
            args.interactive,
        )
        .await?;

    std::process::exit(exit_code);
}

/// Execute the `attach` command
pub async fn execute_attach(args: AttachArgs) -> anyhow::Result<()> {
    let paths = DarkerPaths::new()?;
    let container_store = ContainerStore::new(&paths)?;

    // Find container
    let container_id = container_store
        .find(&args.container)
        .ok_or_else(|| DarkerError::ContainerNotFound(args.container.clone()))?;

    let config = container_store.load(&container_id)?;

    // Check if container is running
    let state = container_store.load_state(&container_id)?;
    if !state.running {
        return Err(DarkerError::ContainerNotRunning(args.container).into());
    }

    // Attach to container
    let container = Container::from_config(config, &paths)?;
    container.attach(!args.no_stdin).await?;

    Ok(())
}
