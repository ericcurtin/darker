//! Layer management for OCI images

use crate::storage::paths::DarkerPaths;
use crate::Result;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Layer manager for handling OCI image layers
pub struct LayerManager {
    paths: DarkerPaths,
}

impl LayerManager {
    /// Create a new layer manager
    pub fn new(paths: &DarkerPaths) -> Self {
        Self {
            paths: paths.clone(),
        }
    }

    /// Check if a layer exists
    pub fn exists(&self, digest: &str) -> bool {
        let layer_dir = self.paths.layer_dir(digest);
        layer_dir.exists()
    }

    /// Get the path to a layer tar file
    pub fn layer_tar_path(&self, digest: &str) -> PathBuf {
        self.paths.layer_tar(digest)
    }

    /// Get the path to extracted layer contents
    pub fn layer_extracted_path(&self, digest: &str) -> PathBuf {
        self.paths.layer_extracted(digest)
    }

    /// Store a layer from a reader
    pub async fn store_layer<R: Read>(&self, digest: &str, mut reader: R) -> Result<()> {
        let layer_dir = self.paths.layer_dir(digest);
        fs::create_dir_all(&layer_dir)?;

        let tar_path = self.paths.layer_tar(digest);
        let mut file = File::create(&tar_path)?;

        let mut buffer = [0u8; 8192];
        loop {
            let bytes_read = reader.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            file.write_all(&buffer[..bytes_read])?;
        }

        Ok(())
    }

    /// Store a layer from bytes
    pub fn store_layer_bytes(&self, digest: &str, data: &[u8]) -> Result<()> {
        let layer_dir = self.paths.layer_dir(digest);
        fs::create_dir_all(&layer_dir)?;

        let tar_path = self.paths.layer_tar(digest);
        fs::write(&tar_path, data)?;

        Ok(())
    }

    /// Extract a layer
    pub fn extract_layer(&self, digest: &str) -> Result<PathBuf> {
        let tar_path = self.paths.layer_tar(digest);
        let extracted_path = self.paths.layer_extracted(digest);

        if extracted_path.exists() {
            return Ok(extracted_path);
        }

        fs::create_dir_all(&extracted_path)?;

        // Open the tar file
        let file = File::open(&tar_path)?;

        // Check if it's gzipped
        let mut reader = std::io::BufReader::new(file);
        let mut magic = [0u8; 2];
        reader.read_exact(&mut magic)?;

        // Reopen the file
        let file = File::open(&tar_path)?;

        if magic == [0x1f, 0x8b] {
            // Gzipped
            let decoder = flate2::read::GzDecoder::new(file);
            let mut archive = tar::Archive::new(decoder);
            archive.unpack(&extracted_path)?;
        } else {
            // Plain tar
            let mut archive = tar::Archive::new(file);
            archive.unpack(&extracted_path)?;
        }

        Ok(extracted_path)
    }

    /// Remove a layer
    pub fn remove_layer(&self, digest: &str) -> Result<()> {
        let layer_dir = self.paths.layer_dir(digest);
        if layer_dir.exists() {
            fs::remove_dir_all(&layer_dir)?;
        }
        Ok(())
    }

    /// Compute the SHA256 digest of a file
    pub fn compute_digest(path: &Path) -> Result<String> {
        let mut file = File::open(path)?;
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 8192];

        loop {
            let bytes_read = file.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
        }

        Ok(format!("sha256:{:x}", hasher.finalize()))
    }

    /// Compute the SHA256 digest of bytes
    pub fn compute_digest_bytes(data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        format!("sha256:{:x}", hasher.finalize())
    }

    /// Create a layer from a directory
    pub fn create_layer_from_dir(&self, dir: &Path) -> Result<(String, PathBuf)> {
        // Create a temporary tar file
        let tmp_dir = self.paths.tmp_dir();
        fs::create_dir_all(&tmp_dir)?;
        let tmp_tar = tmp_dir.join(format!("layer_{}.tar", uuid::Uuid::new_v4()));

        // Create tar archive
        let file = File::create(&tmp_tar)?;
        let mut builder = tar::Builder::new(file);
        builder.append_dir_all(".", dir)?;
        builder.finish()?;

        // Compute digest
        let digest = Self::compute_digest(&tmp_tar)?;
        let digest_short = digest.strip_prefix("sha256:").unwrap_or(&digest);

        // Move to layer storage
        let layer_dir = self.paths.layer_dir(digest_short);
        fs::create_dir_all(&layer_dir)?;

        let final_path = self.paths.layer_tar(digest_short);
        fs::rename(&tmp_tar, &final_path)?;

        Ok((digest, final_path))
    }

    /// List all layers
    pub fn list_layers(&self) -> Result<Vec<String>> {
        let layers_dir = self.paths.layers_dir();
        if !layers_dir.exists() {
            return Ok(Vec::new());
        }

        let mut layers = Vec::new();
        for entry in fs::read_dir(layers_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                layers.push(entry.file_name().to_string_lossy().to_string());
            }
        }

        Ok(layers)
    }

    /// Get total size of all layers
    pub fn total_size(&self) -> Result<u64> {
        let layers = self.list_layers()?;
        let mut total = 0u64;

        for layer in layers {
            let tar_path = self.paths.layer_tar(&layer);
            if let Ok(metadata) = fs::metadata(&tar_path) {
                total += metadata.len();
            }
        }

        Ok(total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_layer_manager() {
        let tmp = TempDir::new().unwrap();
        let paths = DarkerPaths::with_root(tmp.path());
        paths.ensure_directories().unwrap();

        let manager = LayerManager::new(&paths);

        // Create test data
        let test_data = b"test layer content";
        let digest = "test123";

        manager.store_layer_bytes(digest, test_data).unwrap();
        assert!(manager.exists(digest));

        manager.remove_layer(digest).unwrap();
        assert!(!manager.exists(digest));
    }

    #[test]
    fn test_compute_digest() {
        let data = b"hello world";
        let digest = LayerManager::compute_digest_bytes(data);
        assert!(digest.starts_with("sha256:"));
    }
}
