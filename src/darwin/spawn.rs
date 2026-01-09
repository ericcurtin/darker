//! posix_spawn wrappers for process creation

use crate::darwin::chroot::can_chroot;
use crate::{DarkerError, Result};
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
        tty: bool,
        interactive: bool,
        log_path: Option<&Path>,
    ) -> Result<i32> {
        if command.is_empty() {
            return Err(DarkerError::Spawn("No command specified".to_string()));
        }

        let use_chroot = can_chroot();
        let cmd_path = &command[0];

        // When using chroot, we use container-relative paths
        // Otherwise, we resolve to host-absolute paths
        let (executable, container_workdir) = if use_chroot {
            // With chroot, use paths relative to container root
            let exec_path = if cmd_path.starts_with('/') {
                cmd_path.clone()
            } else {
                // Search PATH directories
                let search_paths = ["/bin", "/usr/bin", "/usr/local/bin", "/sbin", "/usr/sbin"];
                let found = search_paths
                    .iter()
                    .map(|dir| format!("{}/{}", dir, cmd_path))
                    .find(|p| rootfs.join(p.trim_start_matches('/')).exists());
                found.unwrap_or_else(|| format!("/usr/bin/{}", cmd_path))
            };
            let work = workdir.to_string();
            (exec_path, work)
        } else {
            // Without chroot, resolve to host-absolute paths
            let resolved_bin = if cmd_path.starts_with('/') {
                let container_bin = rootfs.join(cmd_path.trim_start_matches('/'));
                if container_bin.exists() {
                    Some(container_bin.to_string_lossy().to_string())
                } else {
                    None
                }
            } else {
                let search_paths = ["bin", "usr/bin", "usr/local/bin", "sbin", "usr/sbin"];
                search_paths
                    .iter()
                    .map(|dir| rootfs.join(dir).join(cmd_path))
                    .find(|p| p.exists())
                    .map(|p| p.to_string_lossy().to_string())
            };
            let exec_path = resolved_bin.unwrap_or_else(|| cmd_path.clone());
            let work = rootfs.join(workdir.trim_start_matches('/')).to_string_lossy().to_string();
            (exec_path, work)
        };

        let mut cmd = tokio::process::Command::new(&executable);

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
        if !use_chroot {
            if Path::new(&container_workdir).exists() {
                cmd.current_dir(&container_workdir);
            } else {
                cmd.current_dir(rootfs);
            }
        }

        // Set up chroot if running as root
        if use_chroot {
            let rootfs_path = rootfs.to_path_buf();
            let workdir_for_chroot = workdir.to_string();
            unsafe {
                cmd.pre_exec(move || {
                    // chroot to rootfs
                    let path_cstr = std::ffi::CString::new(rootfs_path.to_string_lossy().as_bytes())
                        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "Invalid path"))?;
                    if libc::chroot(path_cstr.as_ptr()) != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    // chdir to workdir
                    let workdir_cstr = std::ffi::CString::new(workdir_for_chroot.as_bytes())
                        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "Invalid workdir"))?;
                    if libc::chdir(workdir_cstr.as_ptr()) != 0 {
                        // Fall back to root if workdir doesn't exist
                        let root_cstr = std::ffi::CString::new("/").unwrap();
                        libc::chdir(root_cstr.as_ptr());
                    }
                    Ok(())
                });
            }
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
        log_path: &Path,
        pid_path: &Path,
    ) -> Result<u32> {
        if command.is_empty() {
            return Err(DarkerError::Spawn("No command specified".to_string()));
        }

        // Build the command string
        let cmd_path = &command[0];

        let resolved_bin = if cmd_path.starts_with('/') {
            // Absolute path - look in rootfs
            let container_bin = rootfs.join(cmd_path.trim_start_matches('/'));
            if container_bin.exists() {
                Some(container_bin)
            } else {
                None
            }
        } else {
            // Relative command - search through PATH directories in rootfs
            let search_paths = ["bin", "usr/bin", "usr/local/bin", "sbin", "usr/sbin"];
            search_paths
                .iter()
                .map(|dir| rootfs.join(dir).join(cmd_path))
                .find(|p| p.exists())
        };

        let actual_cmd = if let Some(bin_path) = resolved_bin {
            bin_path.to_string_lossy().to_string()
        } else {
            cmd_path.clone()
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
        let mut cmd = tokio::process::Command::new("/bin/sh");
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
    // If the string is empty, return empty quotes
    if s.is_empty() {
        return "''".to_string();
    }

    // Use single quotes, which is the safest escaping method
    // Replace any single quotes with '\'' (end quote, escaped quote, start quote)
    let escaped = s.replace('\'', "'\\''");
    format!("'{}'", escaped)
}

/// Low-level posix_spawn wrapper
#[cfg(target_os = "macos")]
pub mod posix {
    use libc::{c_char, pid_t, posix_spawn};
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
