//! What the desktop app keeps between runs (M3 ticket 02):
//!
//! - [`settings`]: a versioned TOML file at `%APPDATA%\Bolo\settings.toml` that maps
//!   onto the engine's [`EngineConfig`](bolo_core::EngineConfig);
//! - [`history`]: recent dictations, text only, in SQLite at
//!   `%LOCALAPPDATA%\Bolo\history.sqlite`;
//! - [`secrets`]: API keys in Windows Credential Manager (service `bolo`). Keys are never
//!   written to a file, a log or a `Debug` string.
//!
//! The user dictionary is a list in the settings; it feeds both the engines and polish.
//! The CLI doesn't use this crate: it keeps reading its keys from the environment.

pub mod history;
pub mod secrets;
pub mod settings;
pub mod usage;

use std::path::PathBuf;

pub use history::{Dictation, History, HistoryRow};
pub use secrets::{PROVIDERS, SERVICE, Secrets};
pub use settings::{
    AppProfile, AudioSettings, HistorySettings, Loaded, OverlaySettings, PolishSettings, PushToTalkKey,
    SETTINGS_VERSION, Settings, SnippetRow, exe_name,
};
pub use usage::{ProviderUsage, UsageEvent, provider_of};

/// Everything that can go wrong in the store. Messages never contain an API key.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("{}: {source}", path.display())]
    Io { path: PathBuf, source: std::io::Error },
    #[error("settings version {0} is newer than this Bolo understands ({SETTINGS_VERSION})")]
    NewerSettings(i64),
    #[error("can't write the settings: {0}")]
    Serialize(#[from] toml::ser::Error),
    #[error("history database: {0}")]
    History(#[from] rusqlite::Error),
    /// The credential store's message (its `Display`, which never holds the secret).
    #[error("credential store: {0}")]
    Secrets(String),
    #[error("unknown API key provider {0:?}")]
    UnknownProvider(String),
    #[error("no {0} directory on this system")]
    NoHome(&'static str),
}

impl From<keyring_core::Error> for StoreError {
    fn from(e: keyring_core::Error) -> Self {
        Self::Secrets(e.to_string())
    }
}

pub type Result<T, E = StoreError> = std::result::Result<T, E>;

fn io_err(path: &std::path::Path) -> impl FnOnce(std::io::Error) -> StoreError + '_ {
    move |source| StoreError::Io { path: path.to_owned(), source }
}

/// `%APPDATA%\Bolo` on Windows (roaming: small files that follow the user).
pub fn config_dir() -> Result<PathBuf> {
    directories::BaseDirs::new().map(|d| d.config_dir().join("Bolo")).ok_or(StoreError::NoHome("config"))
}

/// `%LOCALAPPDATA%\Bolo` on Windows (local: history and models stay on this machine).
pub fn data_local_dir() -> Result<PathBuf> {
    directories::BaseDirs::new().map(|d| d.data_local_dir().join("Bolo")).ok_or(StoreError::NoHome("local data"))
}
