//! posix_spawn wrappers for process creation

use crate::{DarkerError, Result};
use std::ffi::CString;
use std::path::Path;

/// High-level process spawner using posix_spawn
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
        let container_bin = rootfs.join(command[0].trim_start_matches('/'));

        let mut cmd = if container_bin.exists() {
            tokio::process::Command::new(&container_bin)
        } else {
            // Fall back to system command
            tokio::process::Command::new(&command[0])
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
                use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

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

        // Build environment string
        let env_str = env.join(" ");

        // Build the command string
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

        // Use nohup and shell to run in background
        let mut cmd = tokio::process::Command::new("/bin/sh");
        cmd.arg("-c");
        cmd.arg(format!(
            "cd {} && {} {} >> {} 2>&1 & echo $!",
            work_dir.display(),
            env_str,
            if command.len() > 1 {
                format!("{} {}", actual_cmd, command[1..].join(" "))
            } else {
                actual_cmd
            },
            log_path.display()
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

/// Low-level posix_spawn wrapper
#[cfg(target_os = "macos")]
pub mod posix {
    use libc::{c_char, c_int, pid_t, posix_spawn, posix_spawnattr_t};
    use std::ffi::CString;
    use std::ptr;

    /// Spawn a process using posix_spawn
    pub unsafe fn spawn_process(
        path: &str,
        args: &[&str],
        envp: &[&str],
    ) -> Result<pid_t, std::io::Error> {
        let path_c = CString::new(path).map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "Invalid path")
        })?;

        let args_c: Vec<CString> = args
            .iter()
            .map(|s| CString::new(*s).unwrap())
            .collect();
        let mut args_ptrs: Vec<*const c_char> = args_c.iter().map(|s| s.as_ptr()).collect();
        args_ptrs.push(ptr::null());

        let envp_c: Vec<CString> = envp
            .iter()
            .map(|s| CString::new(*s).unwrap())
            .collect();
        let mut envp_ptrs: Vec<*const c_char> = envp_c.iter().map(|s| s.as_ptr()).collect();
        envp_ptrs.push(ptr::null());

        let mut pid: pid_t = 0;
        let result = posix_spawn(
            &mut pid,
            path_c.as_ptr(),
            ptr::null(),
            ptr::null(),
            args_ptrs.as_ptr() as *const *mut c_char,
            envp_ptrs.as_ptr() as *const *mut c_char,
        );

        if result == 0 {
            Ok(pid)
        } else {
            Err(std::io::Error::from_raw_os_error(result))
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub mod posix {
    use libc::pid_t;

    pub unsafe fn spawn_process(
        _path: &str,
        _args: &[&str],
        _envp: &[&str],
    ) -> Result<pid_t, std::io::Error> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "posix_spawn not available on this platform",
        ))
    }
}
