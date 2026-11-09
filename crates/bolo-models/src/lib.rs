//! Model registry (`models/manifest.json`), on-demand downloads with resume + SHA-256
//! verification, and device-tier recommendations.
//!
//! Models are never bundled with the app. The manifest compiled into the binary is the
//! default; a newer signed manifest can be fetched later (M4).

pub mod download;
pub mod manifest;

pub use download::{DownloadProgress, Downloader};
pub use manifest::{Download, Family, License, Manifest, ModelEntry, ModelStatus, Platform, Tier};

#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("manifest is invalid: {0}")]
    Manifest(String),
    #[error("unknown model id `{0}`")]
    UnknownModel(String),
    #[error("model `{0}` is not downloadable yet (status: {1})")]
    NotAvailable(String, String),
    #[error("download failed: {0}")]
    Network(String),
    #[error("HTTP {0} while downloading")]
    Status(u16),
    #[error("checksum mismatch for {file}: expected {expected}, got {actual}")]
    Checksum { file: String, expected: String, actual: String },
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("model files missing in {0}")]
    Missing(String),
}

pub type Result<T, E = ModelError> = std::result::Result<T, E>;
