//! macOS Sandbox (seatbelt) profile generation

use crate::storage::paths::DarkerPaths;
use crate::Result;
use std::path::{Path, PathBuf};

/// Sandbox profile generator for container isolation
pub struct SandboxProfile {
    container_id: String,
    rootfs: PathBuf,
}

impl SandboxProfile {
    /// Create a new sandbox profile
    pub fn new(container_id: &str, rootfs: &Path) -> Result<Self> {
        Ok(Self {
            container_id: container_id.to_string(),
            rootfs: rootfs.to_path_buf(),
        })
    }

    /// Generate the sandbox profile content
    pub fn generate(&self) -> String {
        format!(
            r#"(version 1)
(deny default)

; Allow process execution from container rootfs and system paths
(allow process-exec
    (subpath "{rootfs}")
    (subpath "/usr/bin")
    (subpath "/bin")
    (subpath "/usr/sbin")
    (subpath "/sbin")
    (subpath "/usr/libexec"))

; Allow process forking
(allow process-fork)

; Allow reading system libraries and frameworks
(allow file-read*
    (subpath "/usr/lib")
    (subpath "/System/Library")
    (subpath "/Library/Frameworks")
    (subpath "/usr/share")
    (subpath "/private/var/db")
    (literal "/dev/null")
    (literal "/dev/zero")
    (literal "/dev/random")
    (literal "/dev/urandom"))

; Allow full access to container rootfs
(allow file-read* file-write*
    (subpath "{rootfs}"))

; Allow reading and writing to temp directories
(allow file-read* file-write*
    (subpath "/tmp")
    (subpath "/private/tmp")
    (subpath "/var/folders"))

; Allow network access (host networking)
(allow network*)

; Allow mach services for basic operation
(allow mach-lookup
    (global-name "com.apple.system.logger")
    (global-name "com.apple.system.notification_center")
    (global-name "com.apple.system.DirectoryService.libinfo_v1"))

; Allow sysctl reads for system info
(allow sysctl-read)

; Allow signal handling
(allow signal)

; Allow TTY access
(allow file-read* file-write*
    (regex #"^/dev/ttys[0-9]+$")
    (literal "/dev/tty"))

; Allow pseudo-terminal access
(allow file-read* file-write*
    (regex #"^/dev/pty.*$"))
"#,
            rootfs = self.rootfs.display()
        )
    }

    /// Write the profile to a file and return the path
    pub fn write_profile(&self, paths: &DarkerPaths) -> Result<PathBuf> {
        let profile_dir = paths.container_dir(&self.container_id);
        std::fs::create_dir_all(&profile_dir)?;

        let profile_path = profile_dir.join("sandbox.sb");
        let content = self.generate();
        std::fs::write(&profile_path, content)?;

        Ok(profile_path)
    }
}

/// Minimal sandbox profile for rootless operation
pub fn minimal_profile() -> &'static str {
    r#"(version 1)
(allow default)
"#
}

/// Strict sandbox profile for enhanced isolation
pub fn strict_profile(rootfs: &Path) -> String {
    format!(
        r#"(version 1)
(deny default)

; Allow only essential operations
(allow process-exec (subpath "{rootfs}"))
(allow process-fork)

(allow file-read*
    (subpath "{rootfs}")
    (subpath "/usr/lib")
    (subpath "/System/Library"))

(allow file-read* file-write*
    (subpath "{rootfs}"))

; Deny network access
(deny network*)

; Deny mach services except essential
(deny mach-lookup)
"#,
        rootfs = rootfs.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_profile_generation() {
        let rootfs = PathBuf::from("/test/rootfs");
        let profile = SandboxProfile::new("test123", &rootfs).unwrap();
        let content = profile.generate();

        assert!(content.contains("(version 1)"));
        assert!(content.contains("/test/rootfs"));
        assert!(content.contains("(allow network*)"));
    }
}
