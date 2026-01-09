//! Mount operations for containers

use crate::{DarkerError, Result};
use std::path::Path;

/// Mount types supported
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MountType {
    /// Bind mount
    Bind,
    /// Volume mount
    Volume,
    /// Tmpfs mount
    Tmpfs,
}

/// Mount options
#[derive(Debug, Clone)]
pub struct MountOptions {
    pub mount_type: MountType,
    pub read_only: bool,
    pub propagation: MountPropagation,
}

impl Default for MountOptions {
    fn default() -> Self {
        Self {
            mount_type: MountType::Bind,
            read_only: false,
            propagation: MountPropagation::Private,
        }
    }
}

/// Mount propagation modes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MountPropagation {
    Private,
    Shared,
    Slave,
    Unbindable,
}

/// Mount point representation
#[derive(Debug, Clone)]
pub struct Mount {
    pub source: String,
    pub destination: String,
    pub mount_type: MountType,
    pub options: MountOptions,
}

impl Mount {
    /// Parse a mount specification string (e.g., "/host/path:/container/path:ro")
    pub fn parse(spec: &str) -> Result<Self> {
        let parts: Vec<&str> = spec.split(':').collect();

        if parts.len() < 2 {
            return Err(DarkerError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("Invalid mount specification: {}", spec),
            )));
        }

        let source = parts[0].to_string();
        let destination = parts[1].to_string();
        let read_only = parts.get(2).map(|o| *o == "ro").unwrap_or(false);

        Ok(Self {
            source,
            destination,
            mount_type: MountType::Bind,
            options: MountOptions {
                read_only,
                ..Default::default()
            },
        })
    }

    /// Check if this is a named volume
    pub fn is_named_volume(&self) -> bool {
        !self.source.starts_with('/') && !self.source.starts_with('.')
    }

    /// Check if this is a bind mount
    pub fn is_bind_mount(&self) -> bool {
        self.source.starts_with('/') || self.source.starts_with('.')
    }
}

/// Bind mount helper for rootless containers
/// Since we can't use actual bind mounts without root, we use symlinks
pub fn create_bind_mount(source: &Path, target: &Path, _read_only: bool) -> Result<()> {
    // Create parent directory if needed
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Remove existing target if it's a symlink
    if target.is_symlink() {
        std::fs::remove_file(target)?;
    }

    // Create symlink
    std::os::unix::fs::symlink(source, target)?;

    Ok(())
}

/// Remove a bind mount (symlink)
pub fn remove_bind_mount(target: &Path) -> Result<()> {
    if target.is_symlink() {
        std::fs::remove_file(target)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mount_parse() {
        let mount = Mount::parse("/host/path:/container/path").unwrap();
        assert_eq!(mount.source, "/host/path");
        assert_eq!(mount.destination, "/container/path");
        assert!(!mount.options.read_only);

        let mount_ro = Mount::parse("/host/path:/container/path:ro").unwrap();
        assert!(mount_ro.options.read_only);
    }

    #[test]
    fn test_is_named_volume() {
        let bind = Mount::parse("/host/path:/container/path").unwrap();
        assert!(bind.is_bind_mount());
        assert!(!bind.is_named_volume());

        let volume = Mount {
            source: "my_volume".to_string(),
            destination: "/container/path".to_string(),
            mount_type: MountType::Volume,
            options: Default::default(),
        };
        assert!(volume.is_named_volume());
    }
}
