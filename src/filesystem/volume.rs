//! Volume management for containers

use crate::storage::containers::ContainerStore;
use crate::storage::paths::DarkerPaths;
use crate::{DarkerError, Result};
use serde::{Deserialize, Serialize};
use std::fs;

/// Volume information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Volume {
    pub name: String,
    pub driver: String,
    pub mountpoint: String,
    pub created: chrono::DateTime<chrono::Utc>,
    pub labels: std::collections::HashMap<String, String>,
}

/// Volume manager
pub struct VolumeManager {
    paths: DarkerPaths,
}

impl VolumeManager {
    /// Create a new volume manager
    pub fn new(paths: &DarkerPaths) -> Result<Self> {
        Ok(Self {
            paths: paths.clone(),
        })
    }

    /// Create a new volume
    pub fn create(&self, name: &str) -> Result<Volume> {
        let volume_path = self.paths.volume(name);

        if volume_path.exists() {
            return Err(DarkerError::Io(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                format!("Volume '{}' already exists", name),
            )));
        }

        fs::create_dir_all(&volume_path)?;

        let volume = Volume {
            name: name.to_string(),
            driver: "local".to_string(),
            mountpoint: volume_path.to_string_lossy().to_string(),
            created: chrono::Utc::now(),
            labels: std::collections::HashMap::new(),
        };

        // Write volume metadata
        let metadata_path = volume_path.join("_metadata.json");
        let metadata_json = serde_json::to_string_pretty(&volume)?;
        fs::write(&metadata_path, metadata_json)?;

        Ok(volume)
    }

    /// Get volume by name
    pub fn get(&self, name: &str) -> Result<Volume> {
        let volume_path = self.paths.volume(name);

        if !volume_path.exists() {
            return Err(DarkerError::VolumeNotFound(name.to_string()));
        }

        let metadata_path = volume_path.join("_metadata.json");
        if metadata_path.exists() {
            let metadata_json = fs::read_to_string(&metadata_path)?;
            let volume: Volume = serde_json::from_str(&metadata_json)?;
            Ok(volume)
        } else {
            // Create default metadata for legacy volumes
            Ok(Volume {
                name: name.to_string(),
                driver: "local".to_string(),
                mountpoint: volume_path.to_string_lossy().to_string(),
                created: chrono::Utc::now(),
                labels: std::collections::HashMap::new(),
            })
        }
    }

    /// List all volumes
    pub fn list(&self) -> Result<Vec<Volume>> {
        let volumes_dir = self.paths.volumes_dir();

        if !volumes_dir.exists() {
            return Ok(Vec::new());
        }

        let mut volumes = Vec::new();
        for entry in fs::read_dir(volumes_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                if let Ok(volume) = self.get(&name) {
                    volumes.push(volume);
                }
            }
        }

        Ok(volumes)
    }

    /// Remove a volume
    pub fn remove(&self, name: &str) -> Result<()> {
        let volume_path = self.paths.volume(name);

        if !volume_path.exists() {
            return Err(DarkerError::VolumeNotFound(name.to_string()));
        }

        // Check if volume is in use
        if self.is_in_use(name)? {
            return Err(DarkerError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Volume '{}' is in use", name),
            )));
        }

        fs::remove_dir_all(&volume_path)?;
        Ok(())
    }

    /// Inspect a volume
    pub fn inspect(&self, name: &str) -> Result<serde_json::Value> {
        let volume = self.get(name)?;

        Ok(serde_json::json!({
            "Name": volume.name,
            "Driver": volume.driver,
            "Mountpoint": volume.mountpoint,
            "CreatedAt": volume.created.to_rfc3339(),
            "Labels": volume.labels,
            "Scope": "local",
            "Options": {},
        }))
    }

    /// Check if a volume is in use by any container
    pub fn is_in_use(&self, name: &str) -> Result<bool> {
        let container_store = ContainerStore::new(&self.paths)?;
        let containers = container_store.list()?;

        for container in containers {
            for volume in &container.volumes {
                if volume.starts_with(name) || volume.contains(&format!("/{}:", name)) {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }

    /// Prune unused volumes
    pub fn prune(&self) -> Result<Vec<String>> {
        let volumes = self.list()?;
        let mut removed = Vec::new();

        for volume in volumes {
            if !self.is_in_use(&volume.name)? {
                self.remove(&volume.name)?;
                removed.push(volume.name);
            }
        }

        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_volume_create_and_list() {
        let tmp = TempDir::new().unwrap();
        let paths = DarkerPaths::with_root(tmp.path());
        paths.ensure_directories().unwrap();

        let manager = VolumeManager::new(&paths).unwrap();

        let volume = manager.create("test_volume").unwrap();
        assert_eq!(volume.name, "test_volume");

        let volumes = manager.list().unwrap();
        assert_eq!(volumes.len(), 1);
        assert_eq!(volumes[0].name, "test_volume");
    }

    #[test]
    fn test_volume_remove() {
        let tmp = TempDir::new().unwrap();
        let paths = DarkerPaths::with_root(tmp.path());
        paths.ensure_directories().unwrap();

        let manager = VolumeManager::new(&paths).unwrap();
        manager.create("to_remove").unwrap();

        assert!(manager.list().unwrap().len() == 1);

        manager.remove("to_remove").unwrap();
        assert!(manager.list().unwrap().is_empty());
    }
}
