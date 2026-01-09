//! Container metadata storage

use crate::storage::paths::DarkerPaths;
use crate::{DarkerError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;

/// Container configuration stored on disk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerConfig {
    pub id: String,
    pub name: String,
    pub image: String,
    pub image_id: String,
    pub command: Vec<String>,
    pub entrypoint: Option<String>,
    pub env: Vec<String>,
    pub working_dir: String,
    pub volumes: Vec<String>,
    pub user: Option<String>,
    pub hostname: String,
    pub tty: bool,
    pub stdin_open: bool,
    pub read_only: bool,
    pub auto_remove: bool,
    pub created: DateTime<Utc>,
}

/// Container runtime state
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContainerState {
    pub running: bool,
    pub paused: bool,
    pub pid: Option<u32>,
    pub exit_code: Option<i32>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

impl Default for ContainerConfig {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            image: String::new(),
            image_id: String::new(),
            command: Vec::new(),
            entrypoint: None,
            env: Vec::new(),
            working_dir: "/".to_string(),
            volumes: Vec::new(),
            user: None,
            hostname: String::new(),
            tty: false,
            stdin_open: false,
            read_only: false,
            auto_remove: false,
            created: Utc::now(),
        }
    }
}

/// Container index for quick lookups
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ContainerIndex {
    /// Maps container names to IDs
    names: HashMap<String, String>,
    /// Maps short IDs to full IDs
    short_ids: HashMap<String, String>,
}

/// Manages container metadata storage
pub struct ContainerStore {
    paths: DarkerPaths,
}

impl ContainerStore {
    /// Create a new container store
    pub fn new(paths: &DarkerPaths) -> Result<Self> {
        Ok(Self {
            paths: paths.clone(),
        })
    }

    /// Check if a container exists
    pub fn exists(&self, name: &str) -> bool {
        self.find(name).is_some()
    }

    /// Find a container by name or ID (full or short)
    pub fn find(&self, name_or_id: &str) -> Option<String> {
        // Try to load index
        if let Ok(index) = self.load_index() {
            // Check if it's a name
            if let Some(id) = index.names.get(name_or_id) {
                return Some(id.clone());
            }
            // Check if it's a short ID
            if let Some(id) = index.short_ids.get(name_or_id) {
                return Some(id.clone());
            }
            // Check if it's a full ID
            if index.short_ids.values().any(|id| id == name_or_id) {
                return Some(name_or_id.to_string());
            }
        }

        // Fallback: check if config exists directly
        let config_path = self.paths.container_config(name_or_id);
        if config_path.exists() {
            return Some(name_or_id.to_string());
        }

        None
    }

    /// Create a new container
    pub fn create(&self, config: &ContainerConfig) -> Result<()> {
        // Create container directory
        let container_dir = self.paths.container_dir(&config.id);
        fs::create_dir_all(&container_dir)?;

        // Write config
        let config_path = self.paths.container_config(&config.id);
        let config_json = serde_json::to_string_pretty(config)?;
        fs::write(&config_path, config_json)?;

        // Write initial state
        let state = ContainerState {
            running: false,
            paused: false,
            pid: None,
            exit_code: None,
            started_at: Utc::now(),
            finished_at: None,
        };
        self.save_state(&config.id, &state)?;

        // Update index
        let mut index = self.load_index().unwrap_or_default();
        index.names.insert(config.name.clone(), config.id.clone());
        let short_id = &config.id[..12.min(config.id.len())];
        index
            .short_ids
            .insert(short_id.to_string(), config.id.clone());
        self.save_index(&index)?;

        Ok(())
    }

    /// Load container config
    pub fn load(&self, container_id: &str) -> Result<ContainerConfig> {
        let config_path = self.paths.container_config(container_id);
        let config_json = fs::read_to_string(&config_path).map_err(|_| {
            DarkerError::ContainerNotFound(container_id.to_string())
        })?;
        let config: ContainerConfig = serde_json::from_str(&config_json)?;
        Ok(config)
    }

    /// Load container state
    pub fn load_state(&self, container_id: &str) -> Result<ContainerState> {
        let state_path = self.paths.container_state(container_id);
        if !state_path.exists() {
            return Ok(ContainerState::default());
        }
        let state_json = fs::read_to_string(&state_path)?;
        let state: ContainerState = serde_json::from_str(&state_json)?;
        Ok(state)
    }

    /// Save container state
    pub fn save_state(&self, container_id: &str, state: &ContainerState) -> Result<()> {
        let state_path = self.paths.container_state(container_id);
        let state_json = serde_json::to_string_pretty(state)?;
        fs::write(&state_path, state_json)?;
        Ok(())
    }

    /// Remove a container
    pub fn remove(&self, container_id: &str) -> Result<()> {
        // Load config to get name
        let config = self.load(container_id)?;

        // Remove container directory
        let container_dir = self.paths.container_dir(container_id);
        if container_dir.exists() {
            fs::remove_dir_all(&container_dir)?;
        }

        // Update index
        let mut index = self.load_index().unwrap_or_default();
        index.names.remove(&config.name);
        let short_id = &container_id[..12.min(container_id.len())];
        index.short_ids.remove(short_id);
        self.save_index(&index)?;

        Ok(())
    }

    /// List all containers
    pub fn list(&self) -> Result<Vec<ContainerConfig>> {
        let containers_dir = self.paths.containers_dir();
        if !containers_dir.exists() {
            return Ok(Vec::new());
        }

        let mut containers = Vec::new();
        for entry in fs::read_dir(containers_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let container_id = entry.file_name().to_string_lossy().to_string();
                if let Ok(config) = self.load(&container_id) {
                    containers.push(config);
                }
            }
        }

        Ok(containers)
    }

    fn load_index(&self) -> Result<ContainerIndex> {
        let index_path = self.paths.container_index();
        if !index_path.exists() {
            return Ok(ContainerIndex::default());
        }
        let index_json = fs::read_to_string(&index_path)?;
        let index: ContainerIndex = serde_json::from_str(&index_json)?;
        Ok(index)
    }

    fn save_index(&self, index: &ContainerIndex) -> Result<()> {
        let index_path = self.paths.container_index();
        let index_json = serde_json::to_string_pretty(index)?;
        fs::write(&index_path, index_json)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_container_create_and_load() {
        let tmp = TempDir::new().unwrap();
        let paths = DarkerPaths::with_root(tmp.path());
        paths.ensure_directories().unwrap();

        let store = ContainerStore::new(&paths).unwrap();

        let config = ContainerConfig {
            id: "test123456789".to_string(),
            name: "test-container".to_string(),
            image: "alpine:latest".to_string(),
            image_id: "sha256:abc".to_string(),
            command: vec!["/bin/sh".to_string()],
            ..Default::default()
        };

        store.create(&config).unwrap();

        // Should be findable by name
        assert!(store.exists("test-container"));

        // Should be findable by short ID
        assert!(store.exists("test12345678"));

        // Load and verify
        let loaded = store.load("test123456789").unwrap();
        assert_eq!(loaded.name, "test-container");
    }
}
