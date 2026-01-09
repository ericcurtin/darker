//! Darker - A Docker-like container runtime for macOS
//!
//! This crate provides a container runtime that uses native Darwin APIs
//! for security and isolation, with OCI-compatible image format support.

pub mod cli;
pub mod darwin;
pub mod filesystem;
pub mod image;
pub mod runtime;
pub mod storage;

use thiserror::Error;

/// Main error type for Darker operations
#[derive(Error, Debug)]
pub enum DarkerError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Container not found: {0}")]
    ContainerNotFound(String),

    #[error("Image not found: {0}")]
    ImageNotFound(String),

    #[error("Volume not found: {0}")]
    VolumeNotFound(String),

    #[error("Container already exists: {0}")]
    ContainerExists(String),

    #[error("Container is not running: {0}")]
    ContainerNotRunning(String),

    #[error("Container is already running: {0}")]
    ContainerAlreadyRunning(String),

    #[error("Registry error: {0}")]
    Registry(String),

    #[error("Invalid image reference: {0}")]
    InvalidImageRef(String),

    #[error("Sandbox error: {0}")]
    Sandbox(String),

    #[error("Process spawn error: {0}")]
    Spawn(String),

    #[error("Build error: {0}")]
    Build(String),

    #[error("Layer error: {0}")]
    Layer(String),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("OCI spec error: {0}")]
    OciSpec(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Unsupported operation: {0}")]
    Unsupported(String),
}

pub type Result<T> = std::result::Result<T, DarkerError>;

/// Application version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Application name
pub const APP_NAME: &str = "darker";
