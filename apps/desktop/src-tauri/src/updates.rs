//! Self-update (ADR-0019): which channel a build belongs to, where its feed is, and what
//! the settings window shows. [`Updater`] runs `tauri-plugin-updater` against the chosen
//! channel's feed; everything above it is pure and tested.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use bolo_store::UpdateChannel;
use semver::Version;
use serde::Serialize;

use tauri::AppHandle;

use crate::controller::{AppEvent, Emit};
use crate::lock;

/// The Beta feed: `/releases/latest/` is the newest release (they're not prereleases).
pub const BETA_FEED: &str = "https://github.com/Nirav1810/bolo/releases/latest/download/latest.json";
/// The Nightly feed: the rolling `nightly` release, rebuilt on every push to `main`.
pub const NIGHTLY_FEED: &str = "https://github.com/Nirav1810/bolo/releases/download/nightly/latest.json";
/// How often a running Bolo looks for an update, after the check at launch.
pub const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);
/// The check at launch waits this long, so it doesn't compete with loading the model.
pub const FIRST_CHECK_AFTER: Duration = Duration::from_secs(20);

/// Whether `version` is a nightly build.
pub fn is_nightly(version: &str) -> bool {
    version.contains("-nightly.")
}

/// The channel a build follows until the user picks one.
pub fn build_channel(version: &str) -> UpdateChannel {
    if is_nightly(version) { UpdateChannel::Nightly } else { UpdateChannel::Beta }
}

/// The label next to the version: Nightly for nightlies, Beta for every 0.x release,
/// none from 1.0 on.
pub fn badge(version: &str) -> Option<&'static str> {
    if is_nightly(version) {
        Some("Nightly")
    } else if version.starts_with("0.") {
        Some("Beta")
    } else {
        None
    }
}

/// `Bolo 0.4.0 Beta`, for the tray tooltip.
pub fn title(version: &str) -> String {
    match badge(version) {
        Some(b) => format!("Bolo {version} {b}"),
        None => format!("Bolo {version}"),
    }
}

pub fn feed(channel: UpdateChannel) -> &'static str {
    match channel {
        UpdateChannel::Beta => BETA_FEED,
        UpdateChannel::Nightly => NIGHTLY_FEED,
    }
}

/// Updates only go forward: switching from Nightly back to Beta waits for a release newer
/// than the nightly. `0.4.0-nightly.*` sorts below `0.4.0`, so the release overtakes it.
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
    pub badge: Option<&'static str>,
    pub channel: UpdateChannel,
    pub status: Status,
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

/// This build's version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Runs just before the installer starts and Bolo exits (the plugin exits on Windows):
/// let go of the keyboard and the mic.
pub type BeforeExit = Arc<dyn Fn() + Send + Sync>;

/// Check `channel`'s feed now. Blocking; run it off the main thread. A failed background
/// check is logged only; a `manual` one is shown.
pub fn check(app: &AppHandle, updates: &Updates, channel: UpdateChannel, manual: bool, before_exit: BeforeExit) {
    use tauri_plugin_updater::UpdaterExt;

    if !updates.begin_check() {
        return;
    }
    let url = feed(channel);
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
            tracing::info!(version = %update.version, ?channel, "update available");
            let status = Status::Available { version: update.version.clone(), notes: update.body.clone() };
            updates.offer(Some(&update.version));
            *lock(&updates.pending) = Some(update);
            updates.set(status);
        }
        Ok(None) => {
            tracing::info!(?channel, "up to date");
            *lock(&updates.pending) = None;
            updates.offer(None);
            updates.set(Status::UpToDate);
        }
        Err(message) => {
            tracing::warn!(%message, ?channel, manual, "update check failed");
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
/// (the installer relaunches it). Blocking; run it off the main thread.
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
        || updates.set(Status::Installing),
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

/// The Nightly marker's colour on the tray icon (`--bolo-nightly`, dark-theme value).
pub const NIGHTLY_RGB: [u8; 3] = [0xA7, 0x7B, 0xFF];

/// `Bolo 0.4.0 · Beta`, the tray menu's first (disabled) line.
pub fn menu_header(version: &str) -> String {
    match badge(version) {
        Some(b) => format!("Bolo {version} · {b}"),
        None => format!("Bolo {version}"),
    }
}

/// Mark an RGBA icon as a Nightly: a dot in the bottom-right corner, cut out from the
/// icon by a transparent ring so it reads on any taskbar colour.
pub fn mark_nightly(rgba: &mut [u8], width: u32, height: u32) {
    let size = width.min(height) as f32;
    let radius = size * 0.24;
    let ring = (size * 0.07).max(1.0);
    let (cx, cy) = (width as f32 - radius - 0.5, height as f32 - radius - 0.5);
    for y in 0..height {
        for x in 0..width {
            let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
            let i = ((y * width + x) * 4) as usize;
            let Some(px) = rgba.get_mut(i..i + 4) else { return };
            if d <= radius {
                px.copy_from_slice(&[NIGHTLY_RGB[0], NIGHTLY_RGB[1], NIGHTLY_RGB[2], 255]);
            } else if d <= radius + ring {
                px[3] = 0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex as StdMutex;

    use super::*;

    #[test]
    fn badges_follow_the_version() {
        assert_eq!(badge("0.4.0"), Some("Beta"));
        assert_eq!(badge("0.5.0-nightly.20261111.7"), Some("Nightly"));
        assert_eq!(badge("1.2.0-nightly.20270101.1"), Some("Nightly"));
        assert_eq!(badge("1.0.0"), None);
        assert_eq!(title("0.4.0"), "Bolo 0.4.0 Beta");
        assert_eq!(title("1.0.0"), "Bolo 1.0.0");
    }

    #[test]
    fn the_menu_header_names_the_variant() {
        assert_eq!(menu_header("0.4.0"), "Bolo 0.4.0 · Beta");
        assert_eq!(menu_header("0.5.0-nightly.20261111.7"), "Bolo 0.5.0-nightly.20261111.7 · Nightly");
        assert_eq!(menu_header("1.0.0"), "Bolo 1.0.0");
    }

    #[test]
    fn the_nightly_mark_is_a_dot_in_the_corner_with_a_clear_ring() {
        let (w, h) = (32u32, 32u32);
        let mut icon = vec![10u8; (w * h * 4) as usize];
        mark_nightly(&mut icon, w, h);
        let px = |x: u32, y: u32| &icon[((y * w + x) * 4) as usize..][..4];
        assert_eq!(px(0, 0), &[10, 10, 10, 10], "the rest of the icon is untouched");
        assert_eq!(px(16, 16), &[10, 10, 10, 10]);
        let c = (w as f32 - 32.0 * 0.24 - 0.5) as u32;
        assert_eq!(&px(c, c)[..3], &NIGHTLY_RGB, "the dot");
        assert_eq!(px(c, c)[3], 255);
        let ring = (c as f32 - 32.0 * 0.24 - 1.0) as u32;
        assert_eq!(px(ring, c)[3], 0, "the ring is cut out");
        // A short buffer is left alone rather than panicking.
        mark_nightly(&mut [0u8; 8], 32, 32);
    }

    #[test]
    fn a_build_follows_its_own_channel_until_the_user_picks() {
        assert_eq!(build_channel("0.4.0"), UpdateChannel::Beta);
        assert_eq!(build_channel("0.5.0-nightly.20261111.7"), UpdateChannel::Nightly);
        assert_eq!(feed(UpdateChannel::Beta), BETA_FEED);
        assert_eq!(feed(UpdateChannel::Nightly), NIGHTLY_FEED);
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
        // Back on Beta from a nightly: the older release is not offered.
        assert!(!is_newer(&v("0.5.0-nightly.20261111.1"), &v("0.4.0")));
        assert!(!is_newer(&v("0.4.0"), &v("0.4.0")));
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
