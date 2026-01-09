//! Process spawning utilities

use crate::{DarkerError, Result};
use std::path::Path;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

/// Process spawner for container processes
pub struct ProcessSpawner;

impl ProcessSpawner {
    pub fn new() -> Self {
        Self
    }

    /// Spawn a container process
    pub async fn spawn_container(
        &self,
        command: &[String],
        rootfs: &Path,
        workdir: &str,
        env: &[(String, String)],
        _sandbox_profile: Option<&Path>,
        tty: bool,
        interactive: bool,
        log_path: Option<&Path>,
    ) -> Result<i32> {
        if command.is_empty() {
            return Err(DarkerError::Spawn("No command specified".to_string()));
        }

        // Build the command to run within the container's rootfs
        // Since we can't use chroot without root, we'll run with modified PATH
        let container_bin = rootfs.join(command[0].trim_start_matches('/'));

        let mut cmd = if container_bin.exists() {
            Command::new(&container_bin)
        } else {
            // Fall back to system command
            Command::new(&command[0])
        };

        // Add arguments
        if command.len() > 1 {
            cmd.args(&command[1..]);
        }

        // Set environment
        cmd.env_clear();
        for (key, value) in env {
            cmd.env(key, value);
        }

        // Set working directory
        let container_workdir = rootfs.join(workdir.trim_start_matches('/'));
        if container_workdir.exists() {
            cmd.current_dir(&container_workdir);
        } else {
            cmd.current_dir(rootfs);
        }

        // Configure I/O
        if interactive || tty {
            cmd.stdin(std::process::Stdio::inherit());
            cmd.stdout(std::process::Stdio::inherit());
            cmd.stderr(std::process::Stdio::inherit());

            let mut child = cmd
                .spawn()
                .map_err(|e| DarkerError::Spawn(e.to_string()))?;

            let status = child
                .wait()
                .await
                .map_err(|e| DarkerError::Spawn(e.to_string()))?;

            Ok(status.code().unwrap_or(1))
        } else {
            cmd.stdin(std::process::Stdio::null());
            cmd.stdout(std::process::Stdio::piped());
            cmd.stderr(std::process::Stdio::piped());

            let mut child = cmd
                .spawn()
                .map_err(|e| DarkerError::Spawn(e.to_string()))?;

            // Handle output logging
            if let Some(log_path) = log_path {
                let stdout = child.stdout.take();
                let stderr = child.stderr.take();
                let log_path = log_path.to_path_buf();

                tokio::spawn(async move {
                    let mut log_file = tokio::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&log_path)
                        .await
                        .ok();

                    if let Some(stdout) = stdout {
                        let mut reader = BufReader::new(stdout).lines();
                        while let Ok(Some(line)) = reader.next_line().await {
                            println!("{}", line);
                            if let Some(ref mut f) = log_file {
                                let _ = f.write_all(format!("{}\n", line).as_bytes()).await;
                            }
                        }
                    }
                });

                tokio::spawn(async move {
                    if let Some(stderr) = stderr {
                        let mut reader = BufReader::new(stderr).lines();
                        while let Ok(Some(line)) = reader.next_line().await {
                            eprintln!("{}", line);
                        }
                    }
                });
            }

            let status = child
                .wait()
                .await
                .map_err(|e| DarkerError::Spawn(e.to_string()))?;

            Ok(status.code().unwrap_or(1))
        }
    }

    /// Spawn a detached container process
    pub async fn spawn_detached(
        &self,
        command: &[String],
        rootfs: &Path,
        workdir: &str,
        env: &[String],
        _sandbox_profile: Option<&Path>,
        log_path: &Path,
        pid_path: &Path,
    ) -> Result<u32> {
        if command.is_empty() {
            return Err(DarkerError::Spawn("No command specified".to_string()));
        }

        // Create a wrapper script that will run in background
        let container_bin = rootfs.join(command[0].trim_start_matches('/'));
        let actual_cmd = if container_bin.exists() {
            container_bin.to_string_lossy().to_string()
        } else {
            command[0].clone()
        };

        let container_workdir = rootfs.join(workdir.trim_start_matches('/'));
        let work_dir = if container_workdir.exists() {
            container_workdir
        } else {
            rootfs.to_path_buf()
        };

        // Build shell-escaped command
        let escaped_cmd = shell_escape(&actual_cmd);
        let escaped_args: Vec<String> = command[1..]
            .iter()
            .map(|arg| shell_escape(arg))
            .collect();
        let escaped_workdir = shell_escape(&work_dir.to_string_lossy());
        let escaped_logpath = shell_escape(&log_path.to_string_lossy());

        // Build environment export statements
        let env_exports: Vec<String> = env
            .iter()
            .filter_map(|e| {
                if let Some((key, value)) = e.split_once('=') {
                    Some(format!("export {}={}", shell_escape(key), shell_escape(value)))
                } else {
                    None
                }
            })
            .collect();
        let env_str = if env_exports.is_empty() {
            String::new()
        } else {
            format!("{} && ", env_exports.join(" && "))
        };

        // Use nohup and shell to run in background
        let mut cmd = Command::new("/bin/sh");
        cmd.arg("-c");
        cmd.arg(format!(
            "cd {} && {}{} {} >> {} 2>&1 & echo $!",
            escaped_workdir,
            env_str,
            escaped_cmd,
            escaped_args.join(" "),
            escaped_logpath
        ));

        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let output = cmd
            .output()
            .await
            .map_err(|e| DarkerError::Spawn(e.to_string()))?;

        let pid_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let pid: u32 = pid_str
            .parse()
            .map_err(|_| DarkerError::Spawn(format!("Invalid PID: {}", pid_str)))?;

        // Write PID file
        std::fs::write(pid_path, pid.to_string())?;

        Ok(pid)
    }
}

impl Default for ProcessSpawner {
    fn default() -> Self {
        Self::new()
    }
}

/// Escape a string for safe use in shell commands
fn shell_escape(s: &str) -> String {
    // If the string is empty or contains only safe characters, just quote it
    if s.is_empty() {
        return "''".to_string();
    }

    // Use single quotes, which is the safest escaping method
    // Replace any single quotes with '\'' (end quote, escaped quote, start quote)
    let escaped = s.replace('\'', "'\\''");
    format!("'{}'", escaped)
}
