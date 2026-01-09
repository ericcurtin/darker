//! `darker inspect` command implementation

use crate::storage::containers::ContainerStore;
use crate::storage::images::ImageStore;
use crate::storage::paths::DarkerPaths;
use crate::DarkerError;
use clap::Args;
use serde_json::json;

/// Arguments for the `inspect` command
#[derive(Args)]
pub struct InspectArgs {
    /// Names or IDs of objects to inspect
    pub names: Vec<String>,

    /// Return JSON for specified type
    #[arg(long, value_parser = ["container", "image", "volume"])]
    pub r#type: Option<String>,

    /// Format the output using the given Go template
    #[arg(short, long)]
    pub format: Option<String>,

    /// Display total file sizes if the type is container
    #[arg(short, long)]
    pub size: bool,
}

/// Execute the `inspect` command
pub async fn execute(args: InspectArgs) -> anyhow::Result<()> {
    let paths = DarkerPaths::new()?;
    let container_store = ContainerStore::new(&paths)?;
    let image_store = ImageStore::new(&paths)?;

    let mut results = Vec::new();

    for name in &args.names {
        let obj = match args.r#type.as_deref() {
            Some("container") => inspect_container(&container_store, name)?,
            Some("image") => inspect_image(&image_store, name)?,
            Some("volume") => inspect_volume(&paths, name)?,
            None => {
                // Try to detect type automatically
                if let Ok(obj) = inspect_container(&container_store, name) {
                    obj
                } else if let Ok(obj) = inspect_image(&image_store, name) {
                    obj
                } else {
                    return Err(DarkerError::ContainerNotFound(name.clone()).into());
                }
            }
            _ => unreachable!(),
        };

        results.push(obj);
    }

    // Output as JSON
    let output = serde_json::to_string_pretty(&results)?;
    println!("{}", output);

    Ok(())
}

fn inspect_container(
    store: &ContainerStore,
    name: &str,
) -> crate::Result<serde_json::Value> {
    let container_id = store
        .find(name)
        .ok_or_else(|| DarkerError::ContainerNotFound(name.to_string()))?;

    let config = store.load(&container_id)?;
    let state = store.load_state(&container_id)?;

    Ok(json!({
        "Id": config.id,
        "Created": config.created.to_rfc3339(),
        "Path": config.command.first().unwrap_or(&String::new()),
        "Args": config.command.iter().skip(1).collect::<Vec<_>>(),
        "State": {
            "Status": if state.running { "running" } else { "exited" },
            "Running": state.running,
            "Paused": state.paused,
            "Pid": state.pid,
            "ExitCode": state.exit_code,
            "StartedAt": state.started_at.to_rfc3339(),
            "FinishedAt": state.finished_at.map(|t| t.to_rfc3339()),
        },
        "Image": config.image_id,
        "Name": format!("/{}", config.name),
        "Config": {
            "Hostname": config.hostname,
            "User": config.user,
            "Env": config.env,
            "Cmd": config.command,
            "WorkingDir": config.working_dir,
            "Entrypoint": config.entrypoint,
            "Tty": config.tty,
            "OpenStdin": config.stdin_open,
        },
        "NetworkSettings": {
            "Networks": {
                "host": {
                    "NetworkID": "host"
                }
            }
        },
        "Mounts": config.volumes.iter().map(|v| {
            json!({
                "Type": "bind",
                "Source": v.split(':').next().unwrap_or(""),
                "Destination": v.split(':').nth(1).unwrap_or(""),
            })
        }).collect::<Vec<_>>(),
    }))
}

fn inspect_image(store: &ImageStore, name: &str) -> crate::Result<serde_json::Value> {
    let image_id = store
        .find(name)
        .ok_or_else(|| DarkerError::ImageNotFound(name.to_string()))?;

    let metadata = store.load_metadata(&image_id)?;

    Ok(json!({
        "Id": format!("sha256:{}", metadata.id),
        "RepoTags": [format!("{}:{}", 
            metadata.repository.as_deref().unwrap_or("<none>"),
            metadata.tag.as_deref().unwrap_or("<none>")
        )],
        "RepoDigests": metadata.digest.map(|d| vec![d]).unwrap_or_default(),
        "Created": metadata.created.to_rfc3339(),
        "Size": metadata.size,
        "Architecture": "darwin",
        "Os": "darwin",
        "Config": {
            "Hostname": "",
            "Env": [],
            "Cmd": [],
            "WorkingDir": "",
        },
        "RootFS": {
            "Type": "layers",
            "Layers": metadata.layers,
        },
    }))
}

fn inspect_volume(paths: &DarkerPaths, name: &str) -> crate::Result<serde_json::Value> {
    let volume_path = paths.volume(name);
    if !volume_path.exists() {
        return Err(DarkerError::VolumeNotFound(name.to_string()));
    }

    Ok(json!({
        "Name": name,
        "Driver": "local",
        "Mountpoint": volume_path.to_string_lossy(),
        "Labels": {},
        "Scope": "local",
    }))
}
