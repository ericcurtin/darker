//! Image metadata storage

use crate::image::oci::ImageReference;
use crate::storage::paths::DarkerPaths;
use crate::{DarkerError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;

/// Image metadata stored on disk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageMetadata {
    pub id: String,
    pub repository: Option<String>,
    pub tag: Option<String>,
    pub digest: Option<String>,
    pub created: DateTime<Utc>,
    pub size: u64,
    pub layers: Vec<String>,
    pub parent: Option<String>,
}

/// Image config from OCI spec (simplified)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImageConfig {
    #[serde(default)]
    pub config: ImageConfigDetails,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImageConfigDetails {
    #[serde(rename = "Cmd")]
    pub cmd: Option<Vec<String>>,
    #[serde(rename = "Entrypoint")]
    pub entrypoint: Option<Vec<String>>,
    #[serde(rename = "Env")]
    pub env: Option<Vec<String>>,
    #[serde(rename = "WorkingDir")]
    pub working_dir: Option<String>,
    #[serde(rename = "User")]
    pub user: Option<String>,
    #[serde(rename = "ExposedPorts")]
    pub exposed_ports: Option<HashMap<String, serde_json::Value>>,
    #[serde(rename = "Volumes")]
    pub volumes: Option<HashMap<String, serde_json::Value>>,
    #[serde(rename = "Labels")]
    pub labels: Option<HashMap<String, String>>,
}

impl ImageConfig {
    pub fn cmd(&self) -> Option<Vec<String>> {
        self.config.cmd.clone()
    }

    pub fn entrypoint(&self) -> Option<Vec<String>> {
        self.config.entrypoint.clone()
    }

    pub fn env(&self) -> Option<Vec<String>> {
        self.config.env.clone()
    }

    pub fn working_dir(&self) -> Option<&str> {
        self.config.working_dir.as_deref()
    }

    pub fn user(&self) -> Option<&str> {
        self.config.user.as_deref()
    }
}

/// Image index for quick lookups
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ImageIndex {
    /// Maps "repository:tag" to image IDs
    tags: HashMap<String, String>,
    /// Maps short IDs to full IDs
    short_ids: HashMap<String, String>,
}

/// Manages image metadata storage
pub struct ImageStore {
    paths: DarkerPaths,
}

impl ImageStore {
    /// Create a new image store
    pub fn new(paths: &DarkerPaths) -> Result<Self> {
        Ok(Self {
            paths: paths.clone(),
        })
    }

    /// Find an image by reference
    pub fn find_image(&self, reference: &ImageReference) -> Option<String> {
        let tag_key = format!("{}:{}", reference.repository_with_registry(), reference.tag());
        self.find(&tag_key)
    }

    /// Find an image by name, tag, or ID
    pub fn find(&self, name_or_id: &str) -> Option<String> {
        if let Ok(index) = self.load_index() {
            // Check if it's a tag reference
            if let Some(id) = index.tags.get(name_or_id) {
                return Some(id.clone());
            }

            // Try with :latest
            if !name_or_id.contains(':') {
                let with_latest = format!("{}:latest", name_or_id);
                if let Some(id) = index.tags.get(&with_latest) {
                    return Some(id.clone());
                }
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

        // Check if it's a sha256 digest
        let clean_id = name_or_id.strip_prefix("sha256:").unwrap_or(name_or_id);
        let image_dir = self.paths.image_dir(clean_id);
        if image_dir.exists() {
            return Some(clean_id.to_string());
        }

        None
    }

    /// Store image metadata
    pub fn store(
        &self,
        image_id: &str,
        repository: Option<&str>,
        tag: Option<&str>,
        digest: Option<&str>,
        layers: &[String],
        size: u64,
    ) -> Result<()> {
        // Create image directory
        let image_dir = self.paths.image_dir(image_id);
        fs::create_dir_all(&image_dir)?;

        // Write metadata
        let metadata = ImageMetadata {
            id: image_id.to_string(),
            repository: repository.map(String::from),
            tag: tag.map(String::from),
            digest: digest.map(String::from),
            created: Utc::now(),
            size,
            layers: layers.to_vec(),
            parent: None,
        };

        let metadata_path = self.paths.image_metadata(image_id);
        let metadata_json = serde_json::to_string_pretty(&metadata)?;
        fs::write(&metadata_path, metadata_json)?;

        // Update index
        let mut index = self.load_index().unwrap_or_default();
        if let (Some(repo), Some(t)) = (repository, tag) {
            let tag_key = format!("{}:{}", repo, t);
            index.tags.insert(tag_key, image_id.to_string());
        }
        let short_id = &image_id[..12.min(image_id.len())];
        index
            .short_ids
            .insert(short_id.to_string(), image_id.to_string());
        self.save_index(&index)?;

        Ok(())
    }

    /// Load image metadata
    pub fn load_metadata(&self, image_id: &str) -> Result<ImageMetadata> {
        let metadata_path = self.paths.image_metadata(image_id);
        let metadata_json = fs::read_to_string(&metadata_path)
            .map_err(|_| DarkerError::ImageNotFound(image_id.to_string()))?;
        let metadata: ImageMetadata = serde_json::from_str(&metadata_json)?;
        Ok(metadata)
    }

    /// Load image config
    pub fn load_config(&self, image_id: &str) -> Result<ImageConfig> {
        let config_path = self.paths.image_config(image_id);
        if !config_path.exists() {
            return Ok(ImageConfig::default());
        }
        let config_json = fs::read_to_string(&config_path)?;
        let config: ImageConfig = serde_json::from_str(&config_json)?;
        Ok(config)
    }

    /// Save image config
    pub fn save_config(&self, image_id: &str, config: &ImageConfig) -> Result<()> {
        let config_path = self.paths.image_config(image_id);
        let config_json = serde_json::to_string_pretty(config)?;
        fs::write(&config_path, config_json)?;
        Ok(())
    }

    /// Tag an image
    pub fn tag(&self, image_id: &str, reference: &ImageReference) -> Result<()> {
        // Update metadata
        let mut metadata = self.load_metadata(image_id)?;
        metadata.repository = Some(reference.repository_with_registry());
        metadata.tag = Some(reference.tag().to_string());

        let metadata_path = self.paths.image_metadata(image_id);
        let metadata_json = serde_json::to_string_pretty(&metadata)?;
        fs::write(&metadata_path, metadata_json)?;

        // Update index
        let mut index = self.load_index().unwrap_or_default();
        let tag_key = format!("{}:{}", reference.repository_with_registry(), reference.tag());
        index.tags.insert(tag_key, image_id.to_string());
        self.save_index(&index)?;

        Ok(())
    }

    /// Remove an image
    pub fn remove(&self, image_id: &str, prune_layers: bool) -> Result<()> {
        let metadata = self.load_metadata(image_id)?;

        // Remove image directory
        let image_dir = self.paths.image_dir(image_id);
        if image_dir.exists() {
            fs::remove_dir_all(&image_dir)?;
        }

        // Optionally remove layers
        if prune_layers {
            for layer in &metadata.layers {
                let layer_dir = self.paths.layer_dir(layer);
                if layer_dir.exists() {
                    // Only remove if no other image uses this layer
                    // For simplicity, we'll skip this check for now
                    // In a production implementation, we'd reference count layers
                }
            }
        }

        // Update index
        let mut index = self.load_index().unwrap_or_default();
        if let (Some(repo), Some(tag)) = (&metadata.repository, &metadata.tag) {
            let tag_key = format!("{}:{}", repo, tag);
            index.tags.remove(&tag_key);
        }
        let short_id = &image_id[..12.min(image_id.len())];
        index.short_ids.remove(short_id);
        self.save_index(&index)?;

        Ok(())
    }

    /// List all images
    pub fn list(&self) -> Result<Vec<ImageMetadata>> {
        let images_dir = self.paths.images_dir();
        if !images_dir.exists() {
            return Ok(Vec::new());
        }

        let mut images = Vec::new();
        for entry in fs::read_dir(images_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let image_id = entry.file_name().to_string_lossy().to_string();
                if let Ok(metadata) = self.load_metadata(&image_id) {
                    images.push(metadata);
                }
            }
        }

        Ok(images)
    }

    fn load_index(&self) -> Result<ImageIndex> {
        let index_path = self.paths.image_index();
        if !index_path.exists() {
            return Ok(ImageIndex::default());
        }
        let index_json = fs::read_to_string(&index_path)?;
        let index: ImageIndex = serde_json::from_str(&index_json)?;
        Ok(index)
    }

    fn save_index(&self, index: &ImageIndex) -> Result<()> {
        let index_path = self.paths.image_index();
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
    fn test_image_store() {
        let tmp = TempDir::new().unwrap();
        let paths = DarkerPaths::with_root(tmp.path());
        paths.ensure_directories().unwrap();

        let store = ImageStore::new(&paths).unwrap();

        // Store an image
        store
            .store(
                "abc123456789",
                Some("docker.io/library/alpine"),
                Some("latest"),
                Some("sha256:abc"),
                &["layer1".to_string(), "layer2".to_string()],
                1024,
            )
            .unwrap();

        // Find by tag
        let found = store.find("docker.io/library/alpine:latest");
        assert_eq!(found, Some("abc123456789".to_string()));

        // Find by short ID
        let found = store.find("abc123456789");
        assert_eq!(found, Some("abc123456789".to_string()));
    }
}
