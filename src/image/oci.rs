//! OCI image specification types

use crate::{DarkerError, Result};
use serde::{Deserialize, Serialize};

/// Docker/OCI image reference
#[derive(Debug, Clone)]
pub struct ImageReference {
    /// Registry (e.g., "docker.io", "ghcr.io")
    pub registry: String,
    /// Repository (e.g., "library/alpine", "myuser/myapp")
    pub repository: String,
    /// Tag (e.g., "latest", "3.18")
    pub tag: String,
    /// Digest (optional, e.g., "sha256:...")
    pub digest: Option<String>,
}

impl ImageReference {
    /// Parse an image reference string
    pub fn parse(reference: &str) -> Result<Self> {
        let reference = reference.trim();
        if reference.is_empty() {
            return Err(DarkerError::InvalidImageRef("Empty image reference".to_string()));
        }

        // Handle digest
        let (ref_without_digest, digest) = if reference.contains('@') {
            let parts: Vec<&str> = reference.splitn(2, '@').collect();
            (parts[0], Some(parts.get(1).unwrap_or(&"").to_string()))
        } else {
            (reference, None)
        };

        // Handle tag
        let (ref_without_tag, tag) = if ref_without_digest.contains(':')
            && !ref_without_digest
                .rsplit_once(':')
                .map(|(_, t)| t.contains('/'))
                .unwrap_or(false)
        {
            let parts: Vec<&str> = ref_without_digest.rsplitn(2, ':').collect();
            (parts[1], parts[0].to_string())
        } else {
            (ref_without_digest, "latest".to_string())
        };

        // Handle registry and repository
        let (registry, repository) = if ref_without_tag.contains('/') {
            let first_part = ref_without_tag.split('/').next().unwrap_or("");
            // Check if first part looks like a registry
            if first_part.contains('.') || first_part.contains(':') || first_part == "localhost" {
                let parts: Vec<&str> = ref_without_tag.splitn(2, '/').collect();
                (parts[0].to_string(), parts[1].to_string())
            } else {
                // Docker Hub with username
                ("docker.io".to_string(), ref_without_tag.to_string())
            }
        } else {
            // Docker Hub official image
            (
                "docker.io".to_string(),
                format!("library/{}", ref_without_tag),
            )
        };

        Ok(Self {
            registry,
            repository,
            tag,
            digest,
        })
    }

    /// Get the full repository path with registry
    pub fn repository_with_registry(&self) -> String {
        if self.registry == "docker.io" {
            self.repository.clone()
        } else {
            format!("{}/{}", self.registry, self.repository)
        }
    }

    /// Get the full image name with tag
    pub fn full_name(&self) -> String {
        format!("{}:{}", self.repository_with_registry(), self.tag)
    }

    /// Get the tag
    pub fn tag(&self) -> &str {
        &self.tag
    }

    /// Get the API URL for the registry
    pub fn registry_url(&self) -> String {
        if self.registry == "docker.io" {
            "https://registry-1.docker.io".to_string()
        } else if self.registry.starts_with("localhost") {
            format!("http://{}", self.registry)
        } else {
            format!("https://{}", self.registry)
        }
    }
}

/// OCI Image Manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageManifest {
    pub schema_version: i32,
    pub media_type: Option<String>,
    pub config: Descriptor,
    pub layers: Vec<Descriptor>,
    pub annotations: Option<std::collections::HashMap<String, String>>,
}

/// OCI Content Descriptor
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Descriptor {
    pub media_type: String,
    pub digest: String,
    pub size: i64,
    pub urls: Option<Vec<String>>,
    pub annotations: Option<std::collections::HashMap<String, String>>,
}

/// OCI Image Index (for multi-platform images)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageIndex {
    pub schema_version: i32,
    pub media_type: Option<String>,
    pub manifests: Vec<ManifestDescriptor>,
    pub annotations: Option<std::collections::HashMap<String, String>>,
}

/// Manifest descriptor with platform info
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestDescriptor {
    pub media_type: String,
    pub digest: String,
    pub size: i64,
    pub platform: Option<Platform>,
    pub annotations: Option<std::collections::HashMap<String, String>>,
}

/// Platform specification
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Platform {
    pub architecture: String,
    pub os: String,
    #[serde(default)]
    pub os_version: Option<String>,
    #[serde(default)]
    pub os_features: Option<Vec<String>>,
    #[serde(default)]
    pub variant: Option<String>,
    #[serde(default)]
    pub features: Option<Vec<String>>,
}

/// OCI Image Configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OciImageConfig {
    pub architecture: String,
    pub os: String,
    pub config: Option<ImageConfigSpec>,
    pub rootfs: RootFs,
    pub history: Option<Vec<History>>,
}

/// Image configuration specification
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct ImageConfigSpec {
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub exposed_ports: Option<std::collections::HashMap<String, serde_json::Value>>,
    #[serde(default)]
    pub env: Option<Vec<String>>,
    #[serde(default)]
    pub entrypoint: Option<Vec<String>>,
    #[serde(default)]
    pub cmd: Option<Vec<String>>,
    #[serde(default)]
    pub volumes: Option<std::collections::HashMap<String, serde_json::Value>>,
    #[serde(default)]
    pub working_dir: Option<String>,
    #[serde(default)]
    pub labels: Option<std::collections::HashMap<String, String>>,
    #[serde(default)]
    pub stop_signal: Option<String>,
}

/// Root filesystem specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RootFs {
    #[serde(rename = "type")]
    pub fs_type: String,
    pub diff_ids: Vec<String>,
}

/// Image history entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct History {
    pub created: Option<String>,
    pub created_by: Option<String>,
    pub comment: Option<String>,
    pub empty_layer: Option<bool>,
}

/// Media types
pub mod media_types {
    pub const OCI_IMAGE_MANIFEST: &str = "application/vnd.oci.image.manifest.v1+json";
    pub const OCI_IMAGE_INDEX: &str = "application/vnd.oci.image.index.v1+json";
    pub const OCI_IMAGE_CONFIG: &str = "application/vnd.oci.image.config.v1+json";
    pub const OCI_LAYER_TAR_GZIP: &str = "application/vnd.oci.image.layer.v1.tar+gzip";

    pub const DOCKER_MANIFEST_V2: &str = "application/vnd.docker.distribution.manifest.v2+json";
    pub const DOCKER_MANIFEST_LIST: &str =
        "application/vnd.docker.distribution.manifest.list.v2+json";
    pub const DOCKER_CONTAINER_IMAGE: &str = "application/vnd.docker.container.image.v1+json";
    pub const DOCKER_LAYER_TAR_GZIP: &str =
        "application/vnd.docker.image.rootfs.diff.tar.gzip";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_image() {
        let reference = ImageReference::parse("alpine").unwrap();
        assert_eq!(reference.registry, "docker.io");
        assert_eq!(reference.repository, "library/alpine");
        assert_eq!(reference.tag, "latest");
    }

    #[test]
    fn test_parse_image_with_tag() {
        let reference = ImageReference::parse("alpine:3.18").unwrap();
        assert_eq!(reference.registry, "docker.io");
        assert_eq!(reference.repository, "library/alpine");
        assert_eq!(reference.tag, "3.18");
    }

    #[test]
    fn test_parse_image_with_user() {
        let reference = ImageReference::parse("myuser/myapp:v1.0").unwrap();
        assert_eq!(reference.registry, "docker.io");
        assert_eq!(reference.repository, "myuser/myapp");
        assert_eq!(reference.tag, "v1.0");
    }

    #[test]
    fn test_parse_image_with_registry() {
        let reference = ImageReference::parse("ghcr.io/owner/repo:tag").unwrap();
        assert_eq!(reference.registry, "ghcr.io");
        assert_eq!(reference.repository, "owner/repo");
        assert_eq!(reference.tag, "tag");
    }

    #[test]
    fn test_full_name() {
        let reference = ImageReference::parse("alpine:3.18").unwrap();
        assert_eq!(reference.full_name(), "library/alpine:3.18");

        let reference = ImageReference::parse("ghcr.io/owner/repo:tag").unwrap();
        assert_eq!(reference.full_name(), "ghcr.io/owner/repo:tag");
    }
}
