//! What Bolo keeps between runs, and the app logic both apps build on it (M3 ticket 02,
//! M5 ticket 04). The desktop app and the Android keyboard (`bolo-ffi`) share all of it.
//!
//! - [`settings`]: a versioned TOML file (`settings.toml`) that maps onto the engine's
//!   [`EngineConfig`](bolo_core::EngineConfig);
//! - [`history`] and [`usage`]: recent dictations, text only, and the usage meter, in
//!   SQLite (`history.sqlite`);
//! - [`secrets`]: API keys in a `keyring_core` credential store (service `bolo`): Windows
//!   Credential Manager on the desktop, a Keystore-backed store on Android. Keys are never
//!   written to a file, a log or a `Debug` string;
//! - [`apps`]: the bundled app rules and the user's app profiles, resolved per dictation;
//! - [`form`]: the settings form's tidying and what a change means for a running app;
//! - [`journal`]: recording dictations and usage in history as they happen.
//!
//! Where the files live is a [`Paths`]: the OS directories on the desktop
//! ([`Paths::desktop`]), and the directories Kotlin passes in on Android.
//!
//! The user dictionary is a list in the settings; it feeds both the engines and polish.
//! The CLI doesn't use this crate: it keeps reading its keys from the environment.

pub mod apps;
pub mod form;
pub mod history;
pub mod journal;
pub mod secrets;
pub mod settings;
pub mod usage;

use std::path::PathBuf;

pub use apps::{AppKey, AppKind, AppRules};
pub use history::{Dictation, History, HistoryRow};
pub use journal::Journal;
pub use secrets::{PROVIDERS, SERVICE, Secrets};
pub use settings::{
    AppProfile, AudioSettings, Channel, HistorySettings, Loaded, OverlaySettings, PolishSettings, PushToTalkKey,
    SETTINGS_VERSION, Settings, SnippetRow, UpdateSettings, VariantSettings, exe_name, package_name,
};
pub use usage::{ProviderUsage, UsageEvent, UsageSummary, provider_of};

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

/// Where Bolo keeps its files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    /// `settings.toml`: `%APPDATA%\Bolo` on Windows, `filesDir` on Android.
    pub config_dir: PathBuf,
    /// `history.sqlite`: `%LOCALAPPDATA%\Bolo` on Windows, `filesDir` on Android.
    pub data_dir: PathBuf,
    /// The models root (ADR-007): `%LOCALAPPDATA%\Bolo\data\models` on Windows,
    /// `noBackupFilesDir/models` on Android.
    pub models_dir: PathBuf,
}

impl Paths {
    pub fn settings_file(&self) -> PathBuf {
        self.config_dir.join("settings.toml")
    }

    pub fn history_file(&self) -> PathBuf {
        self.data_dir.join("history.sqlite")
    }

    /// The desktop's directories, from the OS.
    #[cfg(not(target_os = "android"))]
    pub fn desktop() -> Result<Self> {
        Ok(Self {
            config_dir: config_dir()?,
            data_dir: data_local_dir()?,
            models_dir: bolo_models::default_models_root().ok_or(StoreError::NoHome("models"))?,
        })
    }
}

/// `%APPDATA%\Bolo` on Windows (roaming: small files that follow the user).
#[cfg(not(target_os = "android"))]
pub fn config_dir() -> Result<PathBuf> {
    directories::BaseDirs::new().map(|d| d.config_dir().join("Bolo")).ok_or(StoreError::NoHome("config"))
}

/// `%LOCALAPPDATA%\Bolo` on Windows (local: history and models stay on this machine).
#[cfg(not(target_os = "android"))]
pub fn data_local_dir() -> Result<PathBuf> {
    directories::BaseDirs::new().map(|d| d.data_local_dir().join("Bolo")).ok_or(StoreError::NoHome("local data"))
}
