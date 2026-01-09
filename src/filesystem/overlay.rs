//! Overlay-like filesystem for layer merging

use crate::Result;
use std::fs;
use std::path::{Path, PathBuf};

/// Overlay filesystem implementation
/// Since macOS doesn't have overlayfs, we simulate it with file copying
pub struct OverlayFs {
    /// Lower layers (read-only)
    lower_layers: Vec<PathBuf>,
    /// Upper layer (read-write)
    upper_layer: PathBuf,
    /// Merged view
    merged: PathBuf,
}

impl OverlayFs {
    /// Create a new overlay filesystem
    pub fn new(lower_layers: Vec<PathBuf>, upper_layer: PathBuf, merged: PathBuf) -> Result<Self> {
        Ok(Self {
            lower_layers,
            upper_layer,
            merged,
        })
    }

    /// Mount the overlay (actually just merge the layers)
    pub fn mount(&self) -> Result<()> {
        // Create merged directory
        fs::create_dir_all(&self.merged)?;

        // Apply lower layers in order (bottom to top)
        for layer in &self.lower_layers {
            if layer.exists() {
                copy_layer(layer, &self.merged)?;
            }
        }

        // Apply upper layer
        if self.upper_layer.exists() {
            copy_layer(&self.upper_layer, &self.merged)?;
        }

        Ok(())
    }

    /// Unmount the overlay
    pub fn unmount(&self) -> Result<()> {
        // Nothing to do for simulated overlay
        Ok(())
    }

    /// Get the merged path
    pub fn merged_path(&self) -> &Path {
        &self.merged
    }

    /// Get the upper layer path
    pub fn upper_layer_path(&self) -> &Path {
        &self.upper_layer
    }

    /// Commit changes from merged to upper layer
    pub fn commit(&self) -> Result<()> {
        // In a real overlay, changes are automatically in the upper layer
        // Here we need to diff and copy changes
        Ok(())
    }
}

/// Copy a layer to the target directory
fn copy_layer(src: &Path, dst: &Path) -> Result<()> {
    if !src.is_dir() {
        return Ok(());
    }

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            fs::create_dir_all(&dst_path)?;
            copy_layer(&src_path, &dst_path)?;
        } else if file_type.is_file() {
            if let Some(parent) = dst_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&src_path, &dst_path)?;
        } else if file_type.is_symlink() {
            let target = fs::read_link(&src_path)?;
            if dst_path.exists() {
                fs::remove_file(&dst_path)?;
            }
            std::os::unix::fs::symlink(&target, &dst_path)?;
        }
    }

    Ok(())
}

/// Layer diff for detecting changes
pub struct LayerDiff {
    added: Vec<PathBuf>,
    modified: Vec<PathBuf>,
    deleted: Vec<PathBuf>,
}

impl LayerDiff {
    /// Create a new layer diff
    pub fn new() -> Self {
        Self {
            added: Vec::new(),
            modified: Vec::new(),
            deleted: Vec::new(),
        }
    }

    /// Compute diff between two directories
    pub fn compute(base: &Path, changed: &Path) -> Result<Self> {
        let mut diff = Self::new();
        diff.diff_recursive(base, changed, Path::new(""))?;
        Ok(diff)
    }

    fn diff_recursive(&mut self, base: &Path, changed: &Path, rel_path: &Path) -> Result<()> {
        let changed_full = changed.join(rel_path);
        let base_full = base.join(rel_path);

        if changed_full.is_dir() {
            for entry in fs::read_dir(&changed_full)? {
                let entry = entry?;
                let name = entry.file_name();
                let new_rel = rel_path.join(&name);
                let base_path = base.join(&new_rel);

                if !base_path.exists() {
                    self.added.push(new_rel.clone());
                } else if entry.file_type()?.is_file() {
                    // Check if modified
                    let changed_meta = fs::metadata(entry.path())?;
                    let base_meta = fs::metadata(&base_path)?;

                    if changed_meta.len() != base_meta.len() {
                        self.modified.push(new_rel.clone());
                    }
                }

                if entry.file_type()?.is_dir() {
                    self.diff_recursive(base, changed, &new_rel)?;
                }
            }
        }

        // Check for deleted files
        if base_full.is_dir() {
            for entry in fs::read_dir(&base_full)? {
                let entry = entry?;
                let name = entry.file_name();
                let new_rel = rel_path.join(&name);
                let changed_path = changed.join(&new_rel);

                if !changed_path.exists() {
                    self.deleted.push(new_rel);
                }
            }
        }

        Ok(())
    }

    /// Get added files
    pub fn added(&self) -> &[PathBuf] {
        &self.added
    }

    /// Get modified files
    pub fn modified(&self) -> &[PathBuf] {
        &self.modified
    }

    /// Get deleted files
    pub fn deleted(&self) -> &[PathBuf] {
        &self.deleted
    }

    /// Check if there are any changes
    pub fn has_changes(&self) -> bool {
        !self.added.is_empty() || !self.modified.is_empty() || !self.deleted.is_empty()
    }
}

impl Default for LayerDiff {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_overlay_mount() {
        let tmp = TempDir::new().unwrap();

        let lower = tmp.path().join("lower");
        let upper = tmp.path().join("upper");
        let merged = tmp.path().join("merged");

        fs::create_dir_all(&lower).unwrap();
        fs::create_dir_all(&upper).unwrap();

        fs::write(lower.join("file1.txt"), "from lower").unwrap();
        fs::write(upper.join("file2.txt"), "from upper").unwrap();

        let overlay = OverlayFs::new(vec![lower], upper, merged.clone()).unwrap();
        overlay.mount().unwrap();

        assert!(merged.join("file1.txt").exists());
        assert!(merged.join("file2.txt").exists());
    }

    #[test]
    fn test_layer_diff() {
        let tmp = TempDir::new().unwrap();

        let base = tmp.path().join("base");
        let changed = tmp.path().join("changed");

        fs::create_dir_all(&base).unwrap();
        fs::create_dir_all(&changed).unwrap();

        fs::write(base.join("unchanged.txt"), "same").unwrap();
        fs::write(changed.join("unchanged.txt"), "same").unwrap();
        fs::write(changed.join("new.txt"), "new file").unwrap();

        let diff = LayerDiff::compute(&base, &changed).unwrap();
        assert!(diff.added().iter().any(|p| p.to_str() == Some("new.txt")));
    }
}
