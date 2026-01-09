//! Docker Registry HTTP API V2 client

use crate::image::layer::LayerManager;
use crate::image::oci::{ImageIndex, ImageManifest, ImageReference, OciImageConfig};
use crate::storage::images::ImageStore;
use crate::storage::paths::DarkerPaths;
use crate::{DarkerError, Result};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION};
use serde::Deserialize;
use std::fs;

/// Registry client for pulling and pushing images
pub struct RegistryClient {
    client: reqwest::Client,
}

impl RegistryClient {
    /// Create a new registry client
    pub fn new() -> Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent("darker/0.1.0")
            .build()
            .map_err(|e| DarkerError::Http(e))?;

        Ok(Self { client })
    }

    /// Pull an image from a registry
    pub async fn pull(&self, reference: &ImageReference, paths: &DarkerPaths) -> Result<String> {
        // Get authentication token
        let token = self.get_auth_token(reference).await?;

        // Fetch manifest
        let manifest = self.fetch_manifest(reference, &token).await?;

        // Fetch config
        let config = self.fetch_config(reference, &manifest.config.digest, &token).await?;

        // Pull layers
        let layer_manager = LayerManager::new(paths);
        let mut layer_digests = Vec::new();
        let mut total_size: u64 = 0;

        for (idx, layer) in manifest.layers.iter().enumerate() {
            let digest = layer.digest.strip_prefix("sha256:").unwrap_or(&layer.digest);

            eprintln!(
                "Pulling layer {}/{}: {}",
                idx + 1,
                manifest.layers.len(),
                &digest[..12]
            );

            if !layer_manager.exists(digest) {
                self.fetch_layer(reference, &layer.digest, &token, paths)
                    .await?;
            }

            layer_digests.push(digest.to_string());
            total_size += layer.size as u64;
        }

        // Calculate image ID from config digest
        let image_id = manifest
            .config
            .digest
            .strip_prefix("sha256:")
            .unwrap_or(&manifest.config.digest)
            .to_string();

        // Store image metadata
        let image_store = ImageStore::new(paths)?;
        image_store.store(
            &image_id,
            Some(&reference.repository_with_registry()),
            Some(&reference.tag),
            Some(&manifest.config.digest),
            &layer_digests,
            total_size,
        )?;

        // Store config
        let config_path = paths.image_config(&image_id);
        let config_json = serde_json::to_string_pretty(&config)?;
        fs::write(&config_path, config_json)?;

        // Store manifest
        let manifest_path = paths.image_manifest(&image_id);
        let manifest_json = serde_json::to_string_pretty(&manifest)?;
        fs::write(&manifest_path, manifest_json)?;

        Ok(image_id)
    }

    /// Push an image to a registry
    pub async fn push(
        &self,
        _reference: &ImageReference,
        _image_id: &str,
        _paths: &DarkerPaths,
    ) -> Result<()> {
        // Push is more complex and requires proper authentication
        // This is a placeholder for the full implementation
        Err(DarkerError::Unsupported(
            "Push is not yet implemented".to_string(),
        ))
    }

    /// Get authentication token for a registry
    async fn get_auth_token(&self, reference: &ImageReference) -> Result<Option<String>> {
        // For Docker Hub, we need to get a token
        if reference.registry == "docker.io" {
            let url = format!(
                "https://auth.docker.io/token?service=registry.docker.io&scope=repository:{}:pull",
                reference.repository
            );

            let response = self.client.get(&url).send().await?;

            if response.status().is_success() {
                let body: TokenResponse = response.json().await?;
                return Ok(Some(body.token));
            }
        }

        // Try anonymous access
        Ok(None)
    }

    /// Fetch image manifest (handles manifest lists/indexes for multi-platform images)
    async fn fetch_manifest(
        &self,
        reference: &ImageReference,
        token: &Option<String>,
    ) -> Result<ImageManifest> {
        let url = format!(
            "{}/v2/{}/manifests/{}",
            reference.registry_url(),
            reference.repository,
            reference.tag
        );

        let mut headers = HeaderMap::new();
        // Accept both single manifests and manifest lists
        headers.insert(
            ACCEPT,
            HeaderValue::from_static(
                "application/vnd.docker.distribution.manifest.v2+json, \
                 application/vnd.oci.image.manifest.v1+json, \
                 application/vnd.docker.distribution.manifest.list.v2+json, \
                 application/vnd.oci.image.index.v1+json",
            ),
        );

        if let Some(token) = token {
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {}", token))
                    .map_err(|_| DarkerError::Registry("Invalid token".to_string()))?,
            );
        }

        let response = self.client.get(&url).headers(headers.clone()).send().await?;

        if !response.status().is_success() {
            return Err(DarkerError::Registry(format!(
                "Failed to fetch manifest: {}",
                response.status()
            )));
        }

        // Get the content type to determine what we received
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        let body = response.text().await?;

        // Check if this is a manifest list/index
        if content_type.contains("manifest.list") || content_type.contains("image.index") {
            let index: ImageIndex = serde_json::from_str(&body)
                .map_err(|e| DarkerError::Registry(format!("Failed to parse manifest list: {}", e)))?;

            // Find the manifest for our platform (macOS/darwin)
            let arch = get_host_arch();
            let platform_manifest = index
                .manifests
                .iter()
                .find(|m| {
                    if let Some(ref platform) = m.platform {
                        // Look for linux (most containers are linux-based) or darwin
                        // Prefer linux since macOS containers aren't common
                        (platform.os == "linux" || platform.os == "darwin")
                            && platform.architecture == arch
                    } else {
                        false
                    }
                })
                .or_else(|| {
                    // Fallback: find any manifest for our arch
                    index.manifests.iter().find(|m| {
                        m.platform
                            .as_ref()
                            .map(|p| p.architecture == arch)
                            .unwrap_or(false)
                    })
                })
                .or_else(|| {
                    // Final fallback: just use the first one
                    index.manifests.first()
                })
                .ok_or_else(|| DarkerError::Registry("No suitable manifest found in index".to_string()))?;

            // Fetch the actual manifest by digest
            let manifest_url = format!(
                "{}/v2/{}/manifests/{}",
                reference.registry_url(),
                reference.repository,
                platform_manifest.digest
            );

            // Update headers to only accept single manifests
            headers.insert(
                ACCEPT,
                HeaderValue::from_static(
                    "application/vnd.docker.distribution.manifest.v2+json, \
                     application/vnd.oci.image.manifest.v1+json",
                ),
            );

            let manifest_response = self.client.get(&manifest_url).headers(headers).send().await?;

            if !manifest_response.status().is_success() {
                return Err(DarkerError::Registry(format!(
                    "Failed to fetch platform manifest: {}",
                    manifest_response.status()
                )));
            }

            let manifest: ImageManifest = manifest_response.json().await?;
            Ok(manifest)
        } else {
            // It's already a single manifest
            let manifest: ImageManifest = serde_json::from_str(&body)
                .map_err(|e| DarkerError::Registry(format!("Failed to parse manifest: {}", e)))?;
            Ok(manifest)
        }
    }

    /// Fetch image config
    async fn fetch_config(
        &self,
        reference: &ImageReference,
        digest: &str,
        token: &Option<String>,
    ) -> Result<OciImageConfig> {
        let url = format!(
            "{}/v2/{}/blobs/{}",
            reference.registry_url(),
            reference.repository,
            digest
        );

        let mut headers = HeaderMap::new();
        if let Some(token) = token {
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {}", token))
                    .map_err(|_| DarkerError::Registry("Invalid token".to_string()))?,
            );
        }

        let response = self.client.get(&url).headers(headers).send().await?;

        if !response.status().is_success() {
            return Err(DarkerError::Registry(format!(
                "Failed to fetch config: {}",
                response.status()
            )));
        }

        let config: OciImageConfig = response.json().await?;
        Ok(config)
    }

    /// Fetch a layer
    async fn fetch_layer(
        &self,
        reference: &ImageReference,
        digest: &str,
        token: &Option<String>,
        paths: &DarkerPaths,
    ) -> Result<()> {
        let url = format!(
            "{}/v2/{}/blobs/{}",
            reference.registry_url(),
            reference.repository,
            digest
        );

        let mut headers = HeaderMap::new();
        if let Some(token) = token {
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {}", token))
                    .map_err(|_| DarkerError::Registry("Invalid token".to_string()))?,
            );
        }

        let response = self.client.get(&url).headers(headers).send().await?;

        if !response.status().is_success() {
            return Err(DarkerError::Registry(format!(
                "Failed to fetch layer: {}",
                response.status()
            )));
        }

        let digest_short = digest.strip_prefix("sha256:").unwrap_or(digest);
        let layer_dir = paths.layer_dir(digest_short);
        fs::create_dir_all(&layer_dir)?;

        let tar_path = paths.layer_tar(digest_short);
        let bytes = response.bytes().await?;

        // Decompress if gzipped
        if bytes.len() >= 2 && bytes[0] == crate::GZIP_MAGIC[0] && bytes[1] == crate::GZIP_MAGIC[1] {
            use flate2::read::GzDecoder;
            use std::io::Read;

            let mut decoder = GzDecoder::new(&bytes[..]);
            let mut decompressed = Vec::new();
            decoder.read_to_end(&mut decompressed)?;
            fs::write(&tar_path, &decompressed)?;
        } else {
            fs::write(&tar_path, &bytes)?;
        }

        Ok(())
    }
}

/// Get the host architecture in OCI format
fn get_host_arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        "arm" => "arm",
        "x86" => "386",
        arch => arch,
    }
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    token: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_client_creation() {
        let client = RegistryClient::new();
        assert!(client.is_ok());
    }
}
