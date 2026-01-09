//! Container lifecycle management

use crate::darwin::spawn::ProcessSpawner;
use crate::runtime::sandbox::SandboxProfile;
use crate::storage::containers::{ContainerConfig, ContainerState, ContainerStore};
use crate::storage::paths::DarkerPaths;
use crate::{DarkerError, Result};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

/// Represents a container instance
pub struct Container {
    config: ContainerConfig,
    paths: DarkerPaths,
    store: ContainerStore,
}

impl Container {
    /// Create a new container from config
    pub fn new(config: ContainerConfig, paths: &DarkerPaths) -> Result<Self> {
        let store = ContainerStore::new(paths)?;
        Ok(Self {
            config,
            paths: paths.clone(),
            store,
        })
    }

    /// Load an existing container
    pub fn from_config(config: ContainerConfig, paths: &DarkerPaths) -> Result<Self> {
        let store = ContainerStore::new(paths)?;
        Ok(Self {
            config,
            paths: paths.clone(),
            store,
        })
    }

    /// Get container ID
    pub fn id(&self) -> &str {
        &self.config.id
    }

    /// Run the container (foreground)
    pub async fn run(&mut self, tty: bool, interactive: bool) -> Result<i32> {
        // Update state to running
        let mut state = self.store.load_state(&self.config.id)?;
        state.running = true;
        state.started_at = chrono::Utc::now();
        self.store.save_state(&self.config.id, &state)?;

        // Build the command
        let rootfs = self.paths.container_rootfs(&self.config.id);
        let log_path = self.paths.container_log(&self.config.id);

        // Prepare environment
        let mut env: Vec<(String, String)> = vec![
            ("HOME".to_string(), "/root".to_string()),
            ("PATH".to_string(), "/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin".to_string()),
            ("TERM".to_string(), std::env::var("TERM").unwrap_or_else(|_| "xterm".to_string())),
            ("HOSTNAME".to_string(), self.config.hostname.clone()),
        ];

        // Add user-specified environment variables
        for env_str in &self.config.env {
            if let Some((key, value)) = env_str.split_once('=') {
                env.push((key.to_string(), value.to_string()));
            }
        }

        // Build command with entrypoint
        let mut full_cmd = Vec::new();
        if let Some(ref entrypoint) = self.config.entrypoint {
            full_cmd.push(entrypoint.clone());
        }
        full_cmd.extend(self.config.command.clone());

        if full_cmd.is_empty() {
            full_cmd.push("/bin/sh".to_string());
        }

        // Create sandbox profile
        let sandbox = SandboxProfile::new(&self.config.id, &rootfs)?;
        let profile_path = sandbox.write_profile(&self.paths)?;

        // Spawn process
        let spawner = ProcessSpawner::new();
        let exit_code = spawner
            .spawn_container(
                &full_cmd,
                &rootfs,
                &self.config.working_dir,
                &env,
                Some(&profile_path),
                tty,
                interactive,
                Some(&log_path),
            )
            .await?;

        // Update state
        state.running = false;
        state.exit_code = Some(exit_code);
        state.finished_at = Some(chrono::Utc::now());
        state.pid = None;
        self.store.save_state(&self.config.id, &state)?;

        Ok(exit_code)
    }

    /// Start container in detached mode
    pub async fn start_detached(&mut self) -> Result<()> {
        let rootfs = self.paths.container_rootfs(&self.config.id);
        let log_path = self.paths.container_log(&self.config.id);
        let pid_path = self.paths.container_pid(&self.config.id);

        // Prepare environment
        let mut env_args = Vec::new();
        env_args.push(format!("HOME=/root"));
        env_args.push(format!("PATH=/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"));
        env_args.push(format!("HOSTNAME={}", self.config.hostname));
        for e in &self.config.env {
            env_args.push(e.clone());
        }

        // Build command
        let mut full_cmd = Vec::new();
        if let Some(ref entrypoint) = self.config.entrypoint {
            full_cmd.push(entrypoint.clone());
        }
        full_cmd.extend(self.config.command.clone());

        if full_cmd.is_empty() {
            full_cmd.push("/bin/sh".to_string());
        }

        // Create sandbox profile
        let sandbox = SandboxProfile::new(&self.config.id, &rootfs)?;
        let profile_path = sandbox.write_profile(&self.paths)?;

        // Spawn detached process
        let spawner = ProcessSpawner::new();
        let pid = spawner
            .spawn_detached(
                &full_cmd,
                &rootfs,
                &self.config.working_dir,
                &env_args,
                Some(&profile_path),
                &log_path,
                &pid_path,
            )
            .await?;

        // Update state
        let mut state = self.store.load_state(&self.config.id)?;
        state.running = true;
        state.pid = Some(pid);
        state.started_at = chrono::Utc::now();
        self.store.save_state(&self.config.id, &state)?;

        Ok(())
    }

    /// Stop the container
    pub async fn stop(&self, timeout: Option<u64>) -> Result<()> {
        let state = self.store.load_state(&self.config.id)?;

        if !state.running {
            return Ok(());
        }

        if let Some(pid) = state.pid {
            // Send SIGTERM first
            unsafe {
                libc::kill(pid as i32, libc::SIGTERM);
            }

            // Wait for timeout
            let timeout = timeout.unwrap_or(10);
            tokio::time::sleep(tokio::time::Duration::from_secs(timeout)).await;

            // Check if still running, send SIGKILL
            let still_running = unsafe { libc::kill(pid as i32, 0) == 0 };
            if still_running {
                unsafe {
                    libc::kill(pid as i32, libc::SIGKILL);
                }
            }
        }

        // Update state
        let mut state = state;
        state.running = false;
        state.finished_at = Some(chrono::Utc::now());
        state.pid = None;
        self.store.save_state(&self.config.id, &state)?;

        Ok(())
    }

    /// Execute a command in a running container
    pub async fn exec(
        &self,
        command: &[String],
        env: &[String],
        workdir: Option<&str>,
        _user: Option<&str>,
        tty: bool,
        interactive: bool,
    ) -> Result<i32> {
        let state = self.store.load_state(&self.config.id)?;
        if !state.running {
            return Err(DarkerError::ContainerNotRunning(self.config.id.clone()));
        }

        let rootfs = self.paths.container_rootfs(&self.config.id);
        let workdir = workdir.unwrap_or(&self.config.working_dir);

        // Prepare environment
        let mut full_env: Vec<(String, String)> = vec![
            ("HOME".to_string(), "/root".to_string()),
            ("PATH".to_string(), "/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin".to_string()),
            ("TERM".to_string(), std::env::var("TERM").unwrap_or_else(|_| "xterm".to_string())),
            ("HOSTNAME".to_string(), self.config.hostname.clone()),
        ];

        for env_str in &self.config.env {
            if let Some((key, value)) = env_str.split_once('=') {
                full_env.push((key.to_string(), value.to_string()));
            }
        }

        for env_str in env {
            if let Some((key, value)) = env_str.split_once('=') {
                full_env.push((key.to_string(), value.to_string()));
            }
        }

        // Create sandbox profile
        let sandbox = SandboxProfile::new(&self.config.id, &rootfs)?;
        let profile_path = sandbox.write_profile(&self.paths)?;

        // Spawn process
        let spawner = ProcessSpawner::new();
        spawner
            .spawn_container(
                command,
                &rootfs,
                workdir,
                &full_env,
                Some(&profile_path),
                tty,
                interactive,
                None,
            )
            .await
    }

    /// Attach to container's main process
    pub async fn attach(&self, _stdin: bool) -> Result<()> {
        let state = self.store.load_state(&self.config.id)?;
        if !state.running {
            return Err(DarkerError::ContainerNotRunning(self.config.id.clone()));
        }

        // In a full implementation, we'd attach to the process's PTY
        // For now, we'll just tail the log file
        let log_path = self.paths.container_log(&self.config.id);
        if log_path.exists() {
            let file = tokio::fs::File::open(&log_path).await?;
            let mut reader = BufReader::new(file).lines();

            while let Some(line) = reader.next_line().await? {
                println!("{}", line);
            }
        }

        Ok(())
    }
}
