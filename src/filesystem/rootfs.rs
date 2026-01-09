//! Root filesystem setup for containers

use crate::storage::paths::DarkerPaths;
use crate::{DarkerError, Result};
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

/// Root filesystem manager for a container
pub struct RootFs {
    paths: DarkerPaths,
    rootfs_path: PathBuf,
}

impl RootFs {
    /// Create a new RootFs manager
    pub fn new(paths: &DarkerPaths, container_id: &str) -> Result<Self> {
        let rootfs_path = paths.container_rootfs(container_id);
        Ok(Self {
            paths: paths.clone(),
            rootfs_path,
        })
    }

    /// Get the rootfs path
    pub fn path(&self) -> &Path {
        &self.rootfs_path
    }

    /// Set up the root filesystem from image layers
    pub fn setup(&self, image_id: &str, volumes: &[String]) -> Result<()> {
        // Create rootfs directory
        fs::create_dir_all(&self.rootfs_path)?;

        // Create standard container directories
        self.create_standard_dirs()?;

        // Set up shared system directories (symlinks to host)
        self.setup_system_symlinks()?;

        // Apply image layers
        self.apply_layers(image_id)?;

        // Set up volumes
        for volume in volumes {
            self.setup_volume(volume)?;
        }

        Ok(())
    }

    /// Create standard container directories
    fn create_standard_dirs(&self) -> Result<()> {
        let dirs = [
            "etc",
            "tmp",
            "var",
            "var/log",
            "var/run",
            "var/tmp",
            "home",
            "root",
            "proc",
            "dev",
            "opt",
            "usr/local/bin",
        ];

        for dir in dirs {
            let full_path = self.rootfs_path.join(dir);
            fs::create_dir_all(&full_path)?;
        }

        // Create basic device nodes (as regular files since we can't create real ones without root)
        let dev_path = self.rootfs_path.join("dev");
        fs::write(dev_path.join("null"), "")?;
        fs::write(dev_path.join("zero"), "")?;
        fs::write(dev_path.join("random"), "")?;
        fs::write(dev_path.join("urandom"), "")?;

        Ok(())
    }

    /// Set up symlinks to host system directories
    fn setup_system_symlinks(&self) -> Result<()> {
        // System directories to symlink from host (read-only)
        let symlinks = [
            ("bin", "/bin"),
            ("sbin", "/sbin"),
            ("usr/bin", "/usr/bin"),
            ("usr/sbin", "/usr/sbin"),
            ("usr/lib", "/usr/lib"),
            ("usr/libexec", "/usr/libexec"),
            ("usr/share", "/usr/share"),
            ("System", "/System"),
            ("Library/Frameworks", "/Library/Frameworks"),
        ];

        for (container_path, host_path) in symlinks {
            let full_container_path = self.rootfs_path.join(container_path);

            // Create parent directory if needed
            if let Some(parent) = full_container_path.parent() {
                fs::create_dir_all(parent)?;
            }

            // Skip if already exists
            if full_container_path.exists() || full_container_path.is_symlink() {
                continue;
            }

            // Check if host path exists
            if Path::new(host_path).exists() {
                // Use symlink for rootless mode
                // Ignore errors for protected paths - container will use extracted binaries
                if let Err(e) = symlink(host_path, &full_container_path) {
                    // Only warn, don't fail - the container may still work with extracted binaries
                    tracing::debug!(
                        "Could not create symlink {} -> {}: {} (continuing anyway)",
                        container_path, host_path, e
                    );
                }
            }
        }

        Ok(())
    }

    /// Apply image layers to the rootfs
    fn apply_layers(&self, image_id: &str) -> Result<()> {
        // Load image metadata to get layers
        let image_metadata_path = self.paths.image_metadata(image_id);
        if !image_metadata_path.exists() {
            // No image layers to apply
            return Ok(());
        }

        let metadata_json = fs::read_to_string(&image_metadata_path)?;
        let metadata: serde_json::Value = serde_json::from_str(&metadata_json)?;

        if let Some(layers) = metadata.get("layers").and_then(|l| l.as_array()) {
            for layer in layers {
                if let Some(layer_sha) = layer.as_str() {
                    self.apply_layer(layer_sha)?;
                }
            }
        }

        Ok(())
    }

    /// Apply a single layer to the rootfs
    fn apply_layer(&self, layer_sha: &str) -> Result<()> {
        let layer_extracted = self.paths.layer_extracted(layer_sha);

        if layer_extracted.exists() {
            // Copy extracted layer contents to rootfs
            copy_dir_contents(&layer_extracted, &self.rootfs_path)?;
        } else {
            // Try to extract from tar
            let layer_tar = self.paths.layer_tar(layer_sha);
            if layer_tar.exists() {
                // Create extraction directory
                fs::create_dir_all(&layer_extracted)?;

                // Extract tar with error handling for permission issues
                let file = fs::File::open(&layer_tar)?;
                let mut archive = tar::Archive::new(file);

                // Don't preserve permissions/ownership - macOS can't set Linux UIDs
                archive.set_preserve_permissions(false);
                archive.set_unpack_xattrs(false);

                // Unpack each entry individually to handle errors gracefully
                for entry_result in archive.entries()? {
                    let mut entry = match entry_result {
                        Ok(e) => e,
                        Err(e) => {
                            tracing::debug!("Skipping tar entry: {}", e);
                            continue;
                        }
                    };

                    // Get the path
                    let path = match entry.path() {
                        Ok(p) => p.to_path_buf(),
                        Err(e) => {
                            tracing::debug!("Skipping entry with invalid path: {}", e);
                            continue;
                        }
                    };

                    // Skip whiteout files (used by overlayfs)
                    let path_str = path.to_string_lossy();
                    if path_str.contains(".wh.") {
                        continue;
                    }

                    let dest = layer_extracted.join(&path);

                    // Create parent directory
                    if let Some(parent) = dest.parent() {
                        let _ = fs::create_dir_all(parent);
                    }

                    // Try to unpack, ignore errors for special files
                    if let Err(e) = entry.unpack(&dest) {
                        tracing::debug!("Could not unpack {}: {} (continuing)", path.display(), e);
                    }
                }

                // Copy to rootfs
                copy_dir_contents(&layer_extracted, &self.rootfs_path)?;
            }
        }

        Ok(())
    }

    /// Set up a volume mount
    fn setup_volume(&self, volume_spec: &str) -> Result<()> {
        let parts: Vec<&str> = volume_spec.split(':').collect();
        if parts.len() < 2 {
            return Err(DarkerError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("Invalid volume specification: {}", volume_spec),
            )));
        }

        let host_path = Path::new(parts[0]);
        let container_path = parts[1].trim_start_matches('/');
        let full_container_path = self.rootfs_path.join(container_path);

        // Create parent directory if needed
        if let Some(parent) = full_container_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Create symlink to host path
        if !full_container_path.exists() {
            symlink(host_path, &full_container_path)?;
        }

        Ok(())
    }

    /// Clean up the rootfs
    pub fn cleanup(&self) -> Result<()> {
        if self.rootfs_path.exists() {
            // Be careful with symlinks - just remove the rootfs directory
            fs::remove_dir_all(&self.rootfs_path)?;
        }
        Ok(())
    }
}

/// Recursively copy directory contents
fn copy_dir_contents(src: &Path, dst: &Path) -> Result<()> {
    if !src.is_dir() {
        return Ok(());
    }

    for entry in fs::read_dir(src)? {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        let file_type = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };

        if file_type.is_dir() {
            let _ = fs::create_dir_all(&dst_path);
            let _ = copy_dir_contents(&src_path, &dst_path);
        } else if file_type.is_file() {
            // Don't overwrite symlinks
            if dst_path.is_symlink() {
                continue;
            }
            if let Some(parent) = dst_path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            // Ignore copy errors (permission issues, etc.)
            let _ = fs::copy(&src_path, &dst_path);
        } else if file_type.is_symlink() {
            if let Ok(target) = fs::read_link(&src_path) {
                if !dst_path.exists() && !dst_path.is_symlink() {
                    // Ignore symlink creation errors
                    let _ = symlink(&target, &dst_path);
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_create_rootfs() {
        let tmp = TempDir::new().unwrap();
        let paths = DarkerPaths::with_root(tmp.path());
        paths.ensure_directories().unwrap();

        let rootfs = RootFs::new(&paths, "test123").unwrap();
        rootfs.setup("", &[]).unwrap();

        // Check standard directories were created
        assert!(rootfs.path().join("etc").exists());
        assert!(rootfs.path().join("tmp").exists());
        assert!(rootfs.path().join("home").exists());
    }
}
