//! chroot operations for container isolation

use crate::{DarkerError, Result};
use std::path::Path;

/// Check if the current process can perform chroot operations
pub fn can_chroot() -> bool {
    // chroot requires root privileges on macOS
    unsafe { libc::geteuid() == 0 }
}

/// Perform a chroot to the specified directory
/// 
/// # Safety
/// This function changes the root directory of the current process.
/// It requires root privileges and should be used carefully.
#[cfg(target_os = "macos")]
pub fn chroot_to(path: &Path) -> Result<()> {
    if !can_chroot() {
        return Err(DarkerError::PermissionDenied(
            "chroot requires root privileges".to_string(),
        ));
    }

    let path_cstr = std::ffi::CString::new(path.to_string_lossy().as_bytes())
        .map_err(|_| DarkerError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Invalid path for chroot",
        )))?;

    let result = unsafe { libc::chroot(path_cstr.as_ptr()) };
    if result != 0 {
        return Err(DarkerError::Io(std::io::Error::last_os_error()));
    }

    // Change to the new root
    let result = unsafe { libc::chdir(b"/\0".as_ptr() as *const libc::c_char) };
    if result != 0 {
        return Err(DarkerError::Io(std::io::Error::last_os_error()));
    }

    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn chroot_to(path: &Path) -> Result<()> {
    Err(DarkerError::Unsupported(
        "chroot not supported on this platform".to_string(),
    ))
}

/// Set up a pseudo-chroot environment without actually calling chroot
/// This is used for rootless containers
pub fn setup_pseudo_chroot(rootfs: &Path) -> Result<PseudoChroot> {
    PseudoChroot::new(rootfs)
}

/// Pseudo-chroot environment for rootless containers
pub struct PseudoChroot {
    rootfs: std::path::PathBuf,
    original_cwd: std::path::PathBuf,
}

impl PseudoChroot {
    /// Create a new pseudo-chroot environment
    pub fn new(rootfs: &Path) -> Result<Self> {
        let original_cwd = std::env::current_dir()
            .map_err(|e| DarkerError::Io(e))?;

        Ok(Self {
            rootfs: rootfs.to_path_buf(),
            original_cwd,
        })
    }

    /// Get the rootfs path
    pub fn rootfs(&self) -> &Path {
        &self.rootfs
    }

    /// Translate a path from container space to host space
    pub fn translate_path(&self, container_path: &Path) -> std::path::PathBuf {
        if container_path.is_absolute() {
            self.rootfs.join(container_path.strip_prefix("/").unwrap_or(container_path))
        } else {
            self.rootfs.join(container_path)
        }
    }

    /// Check if a path exists in the container
    pub fn path_exists(&self, container_path: &Path) -> bool {
        self.translate_path(container_path).exists()
    }
}

impl Drop for PseudoChroot {
    fn drop(&mut self) {
        // Restore original working directory
        let _ = std::env::set_current_dir(&self.original_cwd);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_pseudo_chroot() {
        let tmp = TempDir::new().unwrap();
        let pseudo = PseudoChroot::new(tmp.path()).unwrap();

        let translated = pseudo.translate_path(Path::new("/bin/sh"));
        assert_eq!(translated, tmp.path().join("bin/sh"));
    }

    #[test]
    fn test_can_chroot() {
        // This will usually be false in tests (non-root)
        let _ = can_chroot();
    }
}
