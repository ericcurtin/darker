//! Dockerfile parser and image builder

use crate::image::layer::LayerManager;
use crate::image::oci::ImageReference;
use crate::image::registry::RegistryClient;
use crate::storage::images::ImageStore;
use crate::storage::paths::DarkerPaths;
use crate::{DarkerError, Result};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Image builder for Dockerfile-based builds
pub struct ImageBuilder {
    paths: DarkerPaths,
}

impl ImageBuilder {
    /// Create a new image builder
    pub fn new(paths: &DarkerPaths) -> Result<Self> {
        Ok(Self {
            paths: paths.clone(),
        })
    }

    /// Build an image from a Dockerfile
    pub async fn build(
        &mut self,
        context_path: &Path,
        dockerfile: &str,
        tag: Option<&str>,
        build_args: &HashMap<String, String>,
        _no_cache: bool,
        _target: Option<&str>,
        verbose: bool,
    ) -> Result<String> {
        let dockerfile_path = context_path.join(dockerfile);
        let content = fs::read_to_string(&dockerfile_path)?;

        // Parse Dockerfile
        let instructions = parse_dockerfile(&content)?;

        if instructions.is_empty() {
            return Err(DarkerError::Build("Empty Dockerfile".to_string()));
        }

        let mut _current_image_id: Option<String> = None;
        let mut env_vars: HashMap<String, String> = build_args.clone();
        let mut workdir = "/".to_string();
        let mut cmd: Option<Vec<String>> = None;
        let mut entrypoint: Option<Vec<String>> = None;
        let mut layers: Vec<String> = Vec::new();

        for instruction in instructions {
            match instruction {
                Instruction::From { image, .. } => {
                    if verbose {
                        eprintln!("Step: FROM {}", image);
                    }

                    // Pull base image if needed
                    let image_ref = ImageReference::parse(&image)?;
                    let image_store = ImageStore::new(&self.paths)?;

                    _current_image_id = match image_store.find_image(&image_ref) {
                        Some(id) => {
                            // Load layers from base image
                            let metadata = image_store.load_metadata(&id)?;
                            layers = metadata.layers;
                            Some(id)
                        }
                        None => {
                            if image != "scratch" {
                                if verbose {
                                    eprintln!("Pulling base image {}...", image);
                                }
                                let registry = RegistryClient::new()?;
                                let id = registry.pull(&image_ref, &self.paths).await?;
                                let metadata = image_store.load_metadata(&id)?;
                                layers = metadata.layers;
                                Some(id)
                            } else {
                                // scratch image
                                None
                            }
                        }
                    };
                }
                Instruction::Run { command } => {
                    if verbose {
                        eprintln!("Step: RUN {}", command);
                    }
                    // In a full implementation, we'd execute this in a container
                    // For now, we'll skip actual execution
                }
                Instruction::Copy { src, dst } => {
                    if verbose {
                        eprintln!("Step: COPY {} {}", src, dst);
                    }

                    // Create a layer from the copied files
                    let layer_manager = LayerManager::new(&self.paths);
                    let tmp_dir = self.paths.tmp_dir().join(uuid::Uuid::new_v4().to_string());
                    fs::create_dir_all(&tmp_dir)?;

                    // Copy files from context
                    let src_path = context_path.join(&src);
                    let dst_path = tmp_dir.join(dst.trim_start_matches('/'));

                    if let Some(parent) = dst_path.parent() {
                        fs::create_dir_all(parent)?;
                    }

                    if src_path.is_dir() {
                        copy_dir_recursive(&src_path, &dst_path)?;
                    } else if src_path.exists() {
                        fs::copy(&src_path, &dst_path)?;
                    }

                    // Create layer
                    let (digest, _) = layer_manager.create_layer_from_dir(&tmp_dir)?;
                    let digest_short = digest.strip_prefix("sha256:").unwrap_or(&digest);
                    layers.push(digest_short.to_string());

                    // Cleanup
                    fs::remove_dir_all(&tmp_dir)?;
                }
                Instruction::Add { src, dst } => {
                    if verbose {
                        eprintln!("Step: ADD {} {}", src, dst);
                    }
                    // Similar to COPY but with URL and tar extraction support
                    // For simplicity, treat as COPY
                }
                Instruction::Env { key, value } => {
                    if verbose {
                        eprintln!("Step: ENV {}={}", key, value);
                    }
                    env_vars.insert(key, value);
                }
                Instruction::Workdir { path } => {
                    if verbose {
                        eprintln!("Step: WORKDIR {}", path);
                    }
                    workdir = path;
                }
                Instruction::Cmd { command } => {
                    if verbose {
                        eprintln!("Step: CMD {:?}", command);
                    }
                    cmd = Some(command);
                }
                Instruction::Entrypoint { command } => {
                    if verbose {
                        eprintln!("Step: ENTRYPOINT {:?}", command);
                    }
                    entrypoint = Some(command);
                }
                Instruction::Expose { port } => {
                    if verbose {
                        eprintln!("Step: EXPOSE {}", port);
                    }
                }
                Instruction::User { user } => {
                    if verbose {
                        eprintln!("Step: USER {}", user);
                    }
                }
                Instruction::Label { key, value } => {
                    if verbose {
                        eprintln!("Step: LABEL {}={}", key, value);
                    }
                }
                Instruction::Arg { name, default } => {
                    if !env_vars.contains_key(&name) {
                        if let Some(default) = default {
                            env_vars.insert(name, default);
                        }
                    }
                }
                Instruction::Volume { path } => {
                    if verbose {
                        eprintln!("Step: VOLUME {}", path);
                    }
                }
            }
        }

        // Generate image ID
        let image_id = format!(
            "{:x}",
            Sha256::digest(format!("{:?}{:?}{:?}", layers, cmd, entrypoint).as_bytes())
        );

        // Calculate total size
        let layer_manager = LayerManager::new(&self.paths);
        let mut total_size = 0u64;
        for layer in &layers {
            let tar_path = layer_manager.layer_tar_path(layer);
            if let Ok(metadata) = fs::metadata(&tar_path) {
                total_size += metadata.len();
            }
        }

        // Store image metadata
        let image_store = ImageStore::new(&self.paths)?;
        let (repo, tag_str) = if let Some(tag) = tag {
            let ref_parsed = ImageReference::parse(tag)?;
            (
                Some(ref_parsed.repository_with_registry()),
                Some(ref_parsed.tag),
            )
        } else {
            (None, None)
        };

        image_store.store(
            &image_id,
            repo.as_deref(),
            tag_str.as_deref(),
            None,
            &layers,
            total_size,
        )?;

        // Create image config
        let config = crate::storage::images::ImageConfig {
            config: crate::storage::images::ImageConfigDetails {
                cmd,
                entrypoint,
                env: Some(env_vars.into_iter().map(|(k, v)| format!("{}={}", k, v)).collect()),
                working_dir: Some(workdir),
                ..Default::default()
            },
        };
        image_store.save_config(&image_id, &config)?;

        Ok(image_id)
    }
}

/// Parsed Dockerfile instruction
#[derive(Debug, Clone)]
#[allow(dead_code)] // Some fields are parsed but not yet used
enum Instruction {
    From { image: String, alias: Option<String> },
    Run { command: String },
    Copy { src: String, dst: String },
    Add { src: String, dst: String },
    Env { key: String, value: String },
    Workdir { path: String },
    Cmd { command: Vec<String> },
    Entrypoint { command: Vec<String> },
    Expose { port: String },
    User { user: String },
    Label { key: String, value: String },
    Arg { name: String, default: Option<String> },
    Volume { path: String },
}

/// Parse a Dockerfile into instructions
fn parse_dockerfile(content: &str) -> Result<Vec<Instruction>> {
    let mut instructions = Vec::new();
    let mut current_line = String::new();

    for line in content.lines() {
        let trimmed = line.trim();

        // Skip comments and empty lines
        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }

        // Handle line continuation
        if trimmed.ends_with('\\') {
            current_line.push_str(&trimmed[..trimmed.len() - 1]);
            current_line.push(' ');
            continue;
        }

        current_line.push_str(trimmed);
        let full_line = std::mem::take(&mut current_line);

        // Parse instruction
        let parts: Vec<&str> = full_line.splitn(2, char::is_whitespace).collect();
        if parts.is_empty() {
            continue;
        }

        let instruction = parts[0].to_uppercase();
        let args = parts.get(1).map(|s| s.trim()).unwrap_or("");

        match instruction.as_str() {
            "FROM" => {
                let parts: Vec<&str> = args.split_whitespace().collect();
                let image = parts.first().unwrap_or(&"").to_string();
                let alias = if parts.len() >= 3 && parts[1].to_uppercase() == "AS" {
                    Some(parts[2].to_string())
                } else {
                    None
                };
                instructions.push(Instruction::From { image, alias });
            }
            "RUN" => {
                instructions.push(Instruction::Run {
                    command: args.to_string(),
                });
            }
            "COPY" => {
                let parts: Vec<&str> = args.split_whitespace().collect();
                if parts.len() >= 2 {
                    instructions.push(Instruction::Copy {
                        src: parts[0].to_string(),
                        dst: parts[1].to_string(),
                    });
                }
            }
            "ADD" => {
                let parts: Vec<&str> = args.split_whitespace().collect();
                if parts.len() >= 2 {
                    instructions.push(Instruction::Add {
                        src: parts[0].to_string(),
                        dst: parts[1].to_string(),
                    });
                }
            }
            "ENV" => {
                if let Some((key, value)) = args.split_once('=') {
                    instructions.push(Instruction::Env {
                        key: key.trim().to_string(),
                        value: value.trim().to_string(),
                    });
                } else {
                    let parts: Vec<&str> = args.splitn(2, char::is_whitespace).collect();
                    if parts.len() >= 2 {
                        instructions.push(Instruction::Env {
                            key: parts[0].to_string(),
                            value: parts[1].trim().to_string(),
                        });
                    }
                }
            }
            "WORKDIR" => {
                instructions.push(Instruction::Workdir {
                    path: args.to_string(),
                });
            }
            "CMD" => {
                let command = parse_command_args(args);
                instructions.push(Instruction::Cmd { command });
            }
            "ENTRYPOINT" => {
                let command = parse_command_args(args);
                instructions.push(Instruction::Entrypoint { command });
            }
            "EXPOSE" => {
                instructions.push(Instruction::Expose {
                    port: args.to_string(),
                });
            }
            "USER" => {
                instructions.push(Instruction::User {
                    user: args.to_string(),
                });
            }
            "LABEL" => {
                if let Some((key, value)) = args.split_once('=') {
                    instructions.push(Instruction::Label {
                        key: key.trim().to_string(),
                        value: value.trim().trim_matches('"').to_string(),
                    });
                }
            }
            "ARG" => {
                if let Some((name, default)) = args.split_once('=') {
                    instructions.push(Instruction::Arg {
                        name: name.trim().to_string(),
                        default: Some(default.trim().to_string()),
                    });
                } else {
                    instructions.push(Instruction::Arg {
                        name: args.to_string(),
                        default: None,
                    });
                }
            }
            "VOLUME" => {
                instructions.push(Instruction::Volume {
                    path: args.to_string(),
                });
            }
            _ => {
                // Ignore unknown instructions
            }
        }
    }

    Ok(instructions)
}

/// Parse CMD/ENTRYPOINT arguments
fn parse_command_args(args: &str) -> Vec<String> {
    let trimmed = args.trim();

    // Check if it's JSON array format
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        // Simple JSON array parsing
        let inner = &trimmed[1..trimmed.len() - 1];
        inner
            .split(',')
            .map(|s| s.trim().trim_matches('"').to_string())
            .filter(|s| !s.is_empty())
            .collect()
    } else {
        // Shell format
        vec!["/bin/sh".to_string(), "-c".to_string(), trimmed.to_string()]
    }
}

/// Recursively copy a directory
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_dockerfile() {
        let content = r#"
FROM alpine:3.18
RUN apk add --no-cache curl
COPY . /app
WORKDIR /app
CMD ["./start.sh"]
"#;

        let instructions = parse_dockerfile(content).unwrap();
        assert!(!instructions.is_empty());
    }

    #[test]
    fn test_parse_command_args() {
        let json_args = r#"["./app", "--config", "prod"]"#;
        let parsed = parse_command_args(json_args);
        assert_eq!(parsed, vec!["./app", "--config", "prod"]);

        let shell_args = "echo hello world";
        let parsed = parse_command_args(shell_args);
        assert_eq!(parsed, vec!["/bin/sh", "-c", "echo hello world"]);
    }
}
