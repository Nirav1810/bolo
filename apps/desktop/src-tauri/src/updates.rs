//! Self-update (ADR-0019). Each app follows its own feed: Bolo the releases, Bolo Nightly
//! the nightlies ([`crate::variant`]). [`check`] and [`install`] run
//! `tauri-plugin-updater`; the state above them is pure and tested.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use semver::Version;
use serde::Serialize;
use tauri::AppHandle;

use crate::controller::{AppEvent, Emit};
use crate::lock;
use crate::variant::{self, Variant};

/// How often a running Bolo looks for an update, after the check at launch.
pub const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);
/// The check at launch waits this long, so it doesn't compete with loading the model.
pub const FIRST_CHECK_AFTER: Duration = Duration::from_secs(20);

/// Updates only go forward. `0.4.0-nightly.*` sorts below `0.4.0`, and the first nightly
/// after a release above it.
pub fn is_newer(current: &Version, offered: &Version) -> bool {
    offered > current
}

/// Where the updater is, for the settings window and the tray.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum Status {
    /// Nothing checked yet.
    Idle,
    Checking,
    UpToDate,
    Available {
        version: String,
        notes: Option<String>,
    },
    /// `total` is `None` when the server didn't say.
    Downloading {
        downloaded: u64,
        total: Option<u64>,
    },
    /// Downloaded and verified; the installer runs and Bolo restarts.
    Installing,
    /// This launch is the restart after an update from version `from`.
    Updated {
        from: String,
    },
    /// `manual` is whether the user asked for this check: only then is it shown.
    Error {
        message: String,
        manual: bool,
    },
}

impl Status {
    pub fn available(&self) -> Option<&str> {
        match self {
            Status::Available { version, .. } => Some(version),
            _ => None,
        }
    }

    /// Whether a check may start now (not while one runs or an update installs).
    pub fn can_check(&self) -> bool {
        !matches!(self, Status::Checking | Status::Downloading { .. } | Status::Installing)
    }
}

/// What the About page shows.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct View {
    pub version: String,
    pub product: &'static str,
    pub variant: Variant,
    pub badge: Option<&'static str>,
    /// The other app, to install next to this one.
    pub other: Other,
    pub status: Status,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Other {
    pub product: &'static str,
    pub url: String,
}

impl View {
    pub fn now(status: Status) -> Self {
        let variant = Variant::current();
        Self {
            version: variant::VERSION.into(),
            product: variant.product_name(),
            variant,
            badge: variant::badge(variant::VERSION),
            other: Other { product: variant.other().product_name(), url: variant.other().download_page() },
            status,
        }
    }
}

/// Where [`install`] notes the version it is updating from, just before the installer
/// runs. The installer starts the new Bolo hidden in the tray like any launch, so the new
/// Bolo reads this ([`take_updated`]) to know it should open its window and say so.
/// One per app, since Bolo and Bolo Nightly share the data folder.
pub fn marker_path() -> Option<PathBuf> {
    let name = match Variant::current() {
        Variant::Release => "updating-from.txt",
        Variant::Nightly => "updating-from-nightly.txt",
    };
    bolo_models::default_models_root().and_then(|m| m.parent().map(|d| d.join(name)))
}

/// Note that this app, at version `from`, is about to be replaced.
pub fn mark_updating(marker: &Path, from: &str) {
    if let Err(e) = std::fs::write(marker, from) {
        tracing::warn!(%e, "could not note the update; the new version will start in the tray");
    }
}

/// The version this launch updated from, if it follows an update: the marker is there
/// and names another version (an install that failed leaves the same one). The marker
/// is removed either way.
pub fn take_updated(marker: &Path, current: &str) -> Option<String> {
    let from = std::fs::read_to_string(marker).ok()?;
    let _ = std::fs::remove_file(marker);
    let from = from.trim();
    (!from.is_empty() && from != current).then(|| from.to_owned())
}

/// Told the available version, or `None`, when that changes.
type OnAvailable = Box<dyn Fn(Option<&str>) + Send + Sync>;

/// The update state shared by the check task, the commands and the tray; every change is
/// sent to the webviews as [`AppEvent::Update`].
pub struct Updates {
    status: Mutex<Status>,
    /// The update the last check found, ready to install.
    pending: Mutex<Option<tauri_plugin_updater::Update>>,
    emit: Arc<dyn Emit>,
    /// The version the last check offered, for the tray.
    offered: Mutex<Option<String>>,
    /// Called with the offered version (or `None`) when that changes.
    on_available: OnAvailable,
}

impl Updates {
    pub fn new(emit: Arc<dyn Emit>, on_available: impl Fn(Option<&str>) + Send + Sync + 'static) -> Self {
        Self {
            status: Mutex::new(Status::Idle),
            pending: Mutex::new(None),
            emit,
            offered: Mutex::new(None),
            on_available: Box::new(on_available),
        }
    }

    pub fn status(&self) -> Status {
        lock(&self.status).clone()
    }

    pub fn set(&self, status: Status) {
        let mut current = lock(&self.status);
        if *current == status {
            return;
        }
        *current = status.clone();
        drop(current);
        self.emit.emit(AppEvent::Update { status });
    }

    /// A check finished: it offered `version`, or nothing. The tray follows this, not the
    /// status, so a failed download or check leaves the offer up to retry.
    pub fn offer(&self, version: Option<&str>) {
        let mut offered = lock(&self.offered);
        if offered.as_deref() != version {
            *offered = version.map(str::to_owned);
            drop(offered);
            (self.on_available)(version);
        }
    }

    /// Mark a check as started; `false` if one is running or an update is installing.
    pub fn begin_check(&self) -> bool {
        let mut current = lock(&self.status);
        if !current.can_check() {
            return false;
        }
        *current = Status::Checking;
        drop(current);
        self.emit.emit(AppEvent::Update { status: Status::Checking });
        true
    }
}

/// Runs just before the installer starts and Bolo exits (the plugin exits on Windows):
/// let go of the keyboard and the mic.
pub type BeforeExit = Arc<dyn Fn() + Send + Sync>;

/// Check this app's feed now. Blocking; run it off the main thread. A failed background
/// check is logged only; a `manual` one is shown.
pub fn check(app: &AppHandle, updates: &Updates, manual: bool, before_exit: BeforeExit) {
    use tauri_plugin_updater::UpdaterExt;

    if !updates.begin_check() {
        return;
    }
    let variant = Variant::current();
    let url = variant.feed();
    let found = tauri::async_runtime::block_on(async {
        let endpoint = url.parse().map_err(|e| format!("bad feed URL {url}: {e}"))?;
        let updater = app
            .updater_builder()
            .endpoints(vec![endpoint])
            .map_err(|e| e.to_string())?
            .version_comparator(|current, remote| is_newer(&current, &remote.version))
            .on_before_exit(move || before_exit())
            .build()
            .map_err(|e| e.to_string())?;
        updater.check().await.map_err(|e| e.to_string())
    });
    match found {
        Ok(Some(update)) => {
            tracing::info!(version = %update.version, ?variant, "update available");
            let status = Status::Available { version: update.version.clone(), notes: update.body.clone() };
            updates.offer(Some(&update.version));
            *lock(&updates.pending) = Some(update);
            updates.set(status);
        }
        Ok(None) => {
            tracing::info!(?variant, "up to date");
            *lock(&updates.pending) = None;
            updates.offer(None);
            updates.set(Status::UpToDate);
        }
        Err(message) => {
            tracing::warn!(%message, ?variant, manual, "update check failed");
            // A quiet re-check that fails keeps the update the last one found.
            let pending = lock(&updates.pending)
                .as_ref()
                .map(|u| Status::Available { version: u.version.clone(), notes: u.body.clone() });
            match pending {
                Some(available) if !manual => updates.set(available),
                _ => updates.set(Status::Error { message, manual }),
            }
        }
    }
}

/// Download and verify the pending update, run its installer quietly and restart Bolo
/// (the installer relaunches it, and the new Bolo opens About: [`take_updated`]).
/// Blocking; run it off the main thread.
pub fn install(app: &AppHandle, updates: &Updates) {
    let Some(update) = lock(&updates.pending).clone() else { return };
    tracing::info!(version = %update.version, "installing update");
    let mut downloaded = 0u64;
    let mut shown = 0u64;
    updates.set(Status::Downloading { downloaded: 0, total: None });
    let done = tauri::async_runtime::block_on(update.download_and_install(
        |chunk, total| {
            downloaded += chunk as u64;
            // One event per 2% (or 256 KiB without a total), not one per chunk.
            let step = total.map_or(256 * 1024, |t| (t / 50).max(1));
            if downloaded - shown >= step || total == Some(downloaded) {
                shown = downloaded;
                updates.set(Status::Downloading { downloaded, total });
            }
        },
        || {
            if let Some(marker) = marker_path() {
                mark_updating(&marker, variant::VERSION);
            }
            updates.set(Status::Installing);
        },
    ));
    match done {
        // Not reached on Windows: the plugin exits once the installer starts.
        Ok(()) => app.restart(),
        Err(e) => {
            tracing::warn!(%e, "update install failed");
            updates.set(Status::Error { message: e.to_string(), manual: true });
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex as StdMutex;

    use super::*;

    #[test]
    fn the_about_view_names_this_app_and_the_other() {
        let view = View::now(Status::Idle);
        assert_eq!(view.version, variant::VERSION);
        assert_ne!(view.product, view.other.product);
        assert!(view.other.url.starts_with("https://github.com/Nirav1810/bolo/releases/"));
    }

    #[test]
    fn updates_only_go_forward() {
        let v = |s: &str| Version::parse(s).unwrap();
        // The release that ships the work overtakes its nightlies.
        assert!(is_newer(&v("0.4.0-nightly.20261111.7"), &v("0.4.0")));
        // Nightlies sort by date, then run.
        assert!(is_newer(&v("0.4.0-nightly.20261111.7"), &v("0.4.0-nightly.20261112.1")));
        assert!(is_newer(&v("0.4.0-nightly.20261111.7"), &v("0.4.0-nightly.20261111.10")));
        // The first nightly after a release is newer than it.
        assert!(is_newer(&v("0.4.0"), &v("0.5.0-nightly.20261111.1")));
        assert!(!is_newer(&v("0.5.0-nightly.20261111.1"), &v("0.4.0")));
        assert!(!is_newer(&v("0.4.0"), &v("0.4.0")));
    }

    #[test]
    fn a_launch_after_an_update_knows_the_old_version_once() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("updating-from.txt");
        assert_eq!(take_updated(&marker, "0.4.0"), None, "no marker: an ordinary launch");
        mark_updating(&marker, "0.4.0-nightly.20261111.9");
        assert_eq!(take_updated(&marker, "0.4.0-nightly.20261111.10").as_deref(), Some("0.4.0-nightly.20261111.9"));
        assert_eq!(take_updated(&marker, "0.4.0-nightly.20261111.10"), None, "only the first launch after");
        // The installer failed and the old version started again: not an update.
        mark_updating(&marker, "0.4.0");
        assert_eq!(take_updated(&marker, "0.4.0"), None);
        assert!(!marker.exists());
    }

    #[derive(Default)]
    struct Sent(StdMutex<Vec<AppEvent>>);
    impl Emit for Sent {
        fn emit(&self, event: AppEvent) {
            self.0.lock().unwrap().push(event);
        }
    }

    #[test]
    fn the_tray_follows_what_checks_offer() {
        let sent = Arc::new(Sent::default());
        let tray = Arc::new(StdMutex::new(Vec::new()));
        let t = tray.clone();
        let updates = Updates::new(sent.clone(), move |v| t.lock().unwrap().push(v.map(str::to_owned)));
        assert!(updates.begin_check());
        assert!(!updates.begin_check(), "one check at a time");
        updates.offer(Some("0.4.1"));
        updates.set(Status::Available { version: "0.4.1".into(), notes: None });
        updates.offer(Some("0.4.1"));
        updates.set(Status::Downloading { downloaded: 10, total: Some(100) });
        assert!(!updates.begin_check(), "no check while downloading");
        updates.set(Status::Error { message: "offline".into(), manual: true });
        // The failed download leaves the offer in the tray.
        assert_eq!(*tray.lock().unwrap(), vec![Some("0.4.1".to_owned())]);
        assert!(updates.begin_check(), "a failed download can be retried");
        updates.offer(Some("0.4.2"));
        updates.offer(None);
        assert_eq!(*tray.lock().unwrap(), vec![Some("0.4.1".to_owned()), Some("0.4.2".to_owned()), None]);
        assert_eq!(sent.0.lock().unwrap().len(), 5, "each status change reaches the webviews once");
    }
}
