//! Path management for ~/.darker/ directory structure

use crate::{DarkerError, Result};
use std::path::{Path, PathBuf};

/// Manages paths for darker's filesystem storage
#[derive(Debug, Clone)]
pub struct DarkerPaths {
    root: PathBuf,
}

impl DarkerPaths {
    /// Create a new DarkerPaths instance using the default root (~/.darker/)
    pub fn new() -> Result<Self> {
        let home = dirs::home_dir().ok_or_else(|| {
            DarkerError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Could not determine user home directory. Please ensure HOME environment variable is set.",
            ))
        })?;

        Ok(Self {
            root: home.join(".darker"),
        })
    }

    /// Create a new DarkerPaths instance with a custom root
    pub fn with_root(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    /// Get the root directory
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Ensure all required directories exist
    pub fn ensure_directories(&self) -> Result<()> {
        std::fs::create_dir_all(self.containers_dir())?;
        std::fs::create_dir_all(self.images_dir())?;
        std::fs::create_dir_all(self.layers_dir())?;
        std::fs::create_dir_all(self.volumes_dir())?;
        std::fs::create_dir_all(self.tmp_dir())?;
        Ok(())
    }

    /// Directory containing container data
    pub fn containers_dir(&self) -> PathBuf {
        self.root.join("containers")
    }

    /// Directory for a specific container
    pub fn container_dir(&self, container_id: &str) -> PathBuf {
        self.containers_dir().join(container_id)
    }

    /// Container config file
    pub fn container_config(&self, container_id: &str) -> PathBuf {
        self.container_dir(container_id).join("config.json")
    }

    /// Container state file
    pub fn container_state(&self, container_id: &str) -> PathBuf {
        self.container_dir(container_id).join("state.json")
    }

    /// Container rootfs directory
    pub fn container_rootfs(&self, container_id: &str) -> PathBuf {
        self.container_dir(container_id).join("rootfs")
    }

    /// Container diff directory (for container-specific changes)
    pub fn container_diff(&self, container_id: &str) -> PathBuf {
        self.container_dir(container_id).join("diff")
    }

    /// Container log file
    pub fn container_log(&self, container_id: &str) -> PathBuf {
        self.container_dir(container_id).join("container.log")
    }

    /// Container PID file
    pub fn container_pid(&self, container_id: &str) -> PathBuf {
        self.container_dir(container_id).join("container.pid")
    }

    /// Directory containing image data
    pub fn images_dir(&self) -> PathBuf {
        self.root.join("images")
    }

    /// Directory for a specific image
    pub fn image_dir(&self, image_id: &str) -> PathBuf {
        self.images_dir().join(image_id)
    }

    /// Image manifest file
    pub fn image_manifest(&self, image_id: &str) -> PathBuf {
        self.image_dir(image_id).join("manifest.json")
    }

    /// Image config file
    pub fn image_config(&self, image_id: &str) -> PathBuf {
        self.image_dir(image_id).join("config.json")
    }

    /// Image metadata file (darker-specific)
    pub fn image_metadata(&self, image_id: &str) -> PathBuf {
        self.image_dir(image_id).join("metadata.json")
    }

    /// Directory containing layer data
    pub fn layers_dir(&self) -> PathBuf {
        self.root.join("layers")
    }

    /// Directory for a specific layer
    pub fn layer_dir(&self, layer_sha: &str) -> PathBuf {
        self.layers_dir().join(layer_sha)
    }

    /// Layer tar file
    pub fn layer_tar(&self, layer_sha: &str) -> PathBuf {
        self.layer_dir(layer_sha).join("layer.tar")
    }

    /// Extracted layer directory
    pub fn layer_extracted(&self, layer_sha: &str) -> PathBuf {
        self.layer_dir(layer_sha).join("extracted")
    }

    /// Directory containing volumes
    pub fn volumes_dir(&self) -> PathBuf {
        self.root.join("volumes")
    }

    /// Directory for a specific volume
    pub fn volume(&self, volume_name: &str) -> PathBuf {
        self.volumes_dir().join(volume_name)
    }

    /// Temporary directory for downloads and builds
    pub fn tmp_dir(&self) -> PathBuf {
        self.root.join("tmp")
    }

    /// Image index file (maps tags to image IDs)
    pub fn image_index(&self) -> PathBuf {
        self.root.join("images.json")
    }

    /// Container index file (maps names to container IDs)
    pub fn container_index(&self) -> PathBuf {
        self.root.join("containers.json")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_paths_structure() {
        let tmp = TempDir::new().unwrap();
        let paths = DarkerPaths::with_root(tmp.path());

        assert_eq!(paths.root(), tmp.path());
        assert_eq!(paths.containers_dir(), tmp.path().join("containers"));
        assert_eq!(paths.images_dir(), tmp.path().join("images"));
        assert_eq!(paths.volumes_dir(), tmp.path().join("volumes"));
    }

    #[test]
    fn test_container_paths() {
        let tmp = TempDir::new().unwrap();
        let paths = DarkerPaths::with_root(tmp.path());

        let container_id = "abc123";
        assert_eq!(
            paths.container_config(container_id),
            tmp.path().join("containers/abc123/config.json")
        );
        assert_eq!(
            paths.container_rootfs(container_id),
            tmp.path().join("containers/abc123/rootfs")
        );
    }

    #[test]
    fn test_ensure_directories() {
        let tmp = TempDir::new().unwrap();
        let paths = DarkerPaths::with_root(tmp.path());

        paths.ensure_directories().unwrap();

        assert!(paths.containers_dir().exists());
        assert!(paths.images_dir().exists());
        assert!(paths.volumes_dir().exists());
        assert!(paths.tmp_dir().exists());
    }
}
