//! Models on the phone (ticket 05): the list for this phone's RAM tier, the
//! recommendation, downloads with progress, cancel and delete.
//!
//! A download runs on the thread that calls [`Bolo::download_model`] (Kotlin's
//! foreground service) through [`bolo_models::Downloader`]: resumable over HTTP Range,
//! SHA-256 verified, unpacked atomically, with the Silero VAD alongside. Progress goes to
//! every `Bolo` in the process on the same models directory (`on_download`), at most
//! every [`PROGRESS_INTERVAL`], and a cancel from any of them stops it. A cancelled
//! download keeps its partial file, and the next one picks up from there.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use bolo_core::LanguageMode;
use bolo_models::{
    DownloadProgress, Downloader, Manifest, ModelEntry, ModelError, ModelView, Phase, Platform, Tier, VAD_MODEL,
};
use tokio::sync::Notify;

use crate::device::{self, Device};
use crate::{Bolo, FfiError, Inner, Language, OPEN, lock};

/// Download progress reaches Kotlin at most this often (10 Hz); phase changes always do.
pub const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

/// The phone's model tier, from its RAM (`tier_thresholds_mb` in the manifest).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum DeviceTier {
    /// ≥ 5000 MiB reported, which every "6 GB" phone is: Nemotron, dual-pass Auto.
    High,
    /// 3–4 GB phones: Moonshine-tiny for English, Whisper-base Hinglish for Hinglish and
    /// Auto. Auto is never dual-pass here.
    Low,
}

impl From<Tier> for DeviceTier {
    fn from(t: Tier) -> Self {
        if t == Tier::AndroidLow { Self::Low } else { Self::High }
    }
}

/// One model as the Models page lists it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    /// `local:<id>`, the engine spec that uses it.
    pub spec: String,
    pub languages: Vec<String>,
    /// Speaks Hindi, and so Hinglish.
    pub hinglish: bool,
    /// The download, in bytes.
    pub download_bytes: u64,
    /// On disk once unpacked, if the manifest knows.
    pub disk_bytes: Option<u64>,
    /// Free space a download needs now: what's left of the archive plus the unpacked
    /// model and 20%, and the VAD if it isn't installed yet.
    pub space_needed: u64,
    pub installed: bool,
    /// The model for this phone and the settings' language.
    pub recommended: bool,
    pub downloading: bool,
    /// The licence's SPDX-style id (`MIT`, `Apache-2.0`, `OpenMDW-1.1`, ...).
    pub license: String,
    pub license_url: String,
    /// The credit the About page shows.
    pub attribution: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum DownloadPhase {
    Downloading,
    /// Checking the SHA-256.
    Verifying,
    Unpacking,
    /// Installed (with the VAD); the next dictation can use it.
    Done,
    /// See `error`. A network failure keeps the partial file for the next attempt.
    Failed,
    /// Stopped by `cancel_download`; the partial file is kept.
    Cancelled,
}

/// A download's progress, as `on_download` delivers it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ModelDownload {
    pub id: String,
    pub phase: DownloadPhase,
    /// Bytes of the archive downloaded, out of `total`.
    pub downloaded: u64,
    pub total: u64,
    /// Why it failed, for `Failed`.
    pub error: Option<String>,
}

impl ModelDownload {
    fn progress(id: &str, p: DownloadProgress) -> Self {
        let phase = match p.phase {
            Phase::Downloading => DownloadPhase::Downloading,
            Phase::Verifying => DownloadPhase::Verifying,
            Phase::Unpacking => DownloadPhase::Unpacking,
            Phase::Done => DownloadPhase::Done,
        };
        Self { id: id.to_owned(), phase, downloaded: p.downloaded, total: p.total, error: None }
    }

    fn end(entry: &ModelEntry, phase: DownloadPhase, error: Option<String>) -> Self {
        let total = entry.download.as_ref().map_or(0, |d| d.size_bytes);
        Self { id: entry.id.clone(), phase, downloaded: 0, total, error }
    }
}

/// Lets `Downloading` progress through at most once per interval. A new phase, the last
/// byte and anything after downloading always pass.
#[derive(Debug)]
pub(crate) struct Throttle {
    interval: Duration,
    last: Option<(Phase, Instant)>,
}

impl Throttle {
    pub fn new(interval: Duration) -> Self {
        Self { interval, last: None }
    }

    pub fn pass(&mut self, p: &DownloadProgress, now: Instant) -> bool {
        let due = match self.last {
            None => true,
            Some((phase, at)) => {
                phase != p.phase
                    || p.phase != Phase::Downloading
                    || p.downloaded >= p.total
                    || now.duration_since(at) >= self.interval
            }
        };
        if due {
            self.last = Some((p.phase, now));
        }
        due
    }
}

/// Stops a running download.
#[doc(hidden)]
#[derive(Debug, Default)]
pub struct CancelToken {
    cancelled: AtomicBool,
    notify: Notify,
}

impl CancelToken {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        // Stores a permit when nothing waits yet, so an early cancel isn't lost.
        self.notify.notify_one();
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    async fn cancelled(&self) {
        if !self.is_cancelled() {
            self.notify.notified().await;
        }
    }
}

/// Installs `entry` (and `vad`) with `downloader` on this thread, reporting progress
/// through `report` at most every `interval`, then `Done`, `Failed` or `Cancelled`.
/// `Ok(true)` once installed, `Ok(false)` when cancelled.
#[doc(hidden)]
pub fn run(
    downloader: &Downloader,
    entry: &ModelEntry,
    vad: Option<&ModelEntry>,
    cancel: &CancelToken,
    interval: Duration,
    report: &(dyn Fn(ModelDownload) + Sync),
) -> Result<bool, ModelError> {
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    let throttle = Mutex::new(Throttle::new(interval));
    let progress = |p: DownloadProgress| {
        // `Done` is sent below, once `install_with_vad` returns.
        if p.phase != Phase::Done && lock(&throttle).pass(&p, Instant::now()) {
            report(ModelDownload::progress(&entry.id, p));
        }
    };
    let result = rt.block_on(async {
        tokio::select! {
            r = downloader.install_with_vad(entry, vad, &progress) => Some(r),
            () = cancel.cancelled() => None,
        }
    });
    match result {
        Some(Ok(_)) => {
            let total = entry.download.as_ref().map_or(0, |d| d.size_bytes);
            report(ModelDownload {
                id: entry.id.clone(),
                phase: DownloadPhase::Done,
                downloaded: total,
                total,
                error: None,
            });
            Ok(true)
        }
        Some(Err(e)) => {
            tracing::warn!(model = %entry.id, %e, "model download failed");
            report(ModelDownload::end(entry, DownloadPhase::Failed, Some(e.to_string())));
            Err(e)
        }
        None => {
            tracing::info!(model = %entry.id, "model download cancelled");
            report(ModelDownload::end(entry, DownloadPhase::Cancelled, None));
            Ok(false)
        }
    }
}

/// The downloads running in this process: models root and id.
static RUNNING: Mutex<Vec<(PathBuf, String, Arc<CancelToken>)>> = Mutex::new(Vec::new());

/// A running download's place in [`RUNNING`], given up when it ends.
struct Running {
    root: PathBuf,
    id: String,
}

impl Running {
    /// `None` when `id` is already downloading into `root`.
    fn begin(root: &Path, id: &str) -> Option<(Self, Arc<CancelToken>)> {
        let mut running = lock(&RUNNING);
        if running.iter().any(|(r, i, _)| r == root && i == id) {
            return None;
        }
        let token = Arc::new(CancelToken::default());
        running.push((root.to_owned(), id.to_owned(), token.clone()));
        Some((Self { root: root.to_owned(), id: id.to_owned() }, token))
    }

    fn token(root: &Path, id: &str) -> Option<Arc<CancelToken>> {
        lock(&RUNNING).iter().find(|(r, i, _)| r == root && i == id).map(|(_, _, t)| t.clone())
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        lock(&RUNNING).retain(|(r, i, _)| !(r == &self.root && i == &self.id));
    }
}

/// The `Bolo`s open on the models in `root`.
fn open_on_models(root: &Path) -> Vec<Arc<Inner>> {
    lock(&OPEN).iter().filter_map(Weak::upgrade).filter(|i| i.paths.models_dir == root).collect()
}

/// The engines to run on this phone. On a low-tier phone, a local model the tier can't
/// run (Nemotron's 1.4 GB, the desktop's models) becomes the tier's model for the
/// language: Moonshine-tiny for English, Whisper-base Hinglish for Hinglish and Auto.
/// So Auto there is the Hinglish model, never dual-pass. Cloud engines, the tier's own
/// models and unknown ids (which fail with a clear error later) stay.
pub(crate) fn fit_engines(
    engines: &[String],
    manifest: &Manifest,
    device: &Device,
    language: LanguageMode,
) -> Vec<String> {
    if device.tier(manifest) != Tier::AndroidLow {
        return engines.to_vec();
    }
    let hinglish = language != LanguageMode::English;
    let fit = manifest.recommend(Platform::Android, device.ram_mb(), hinglish).map(|e| format!("local:{}", e.id));
    let mut out: Vec<String> = Vec::new();
    for spec in engines {
        let too_big = spec
            .strip_prefix("local:")
            .and_then(|id| manifest.get(id).ok())
            .is_some_and(|e| !e.tiers.contains(&Tier::AndroidLow));
        let spec = match (&fit, too_big) {
            (Some(fit), true) => {
                tracing::info!(from = %spec, to = %fit, "this phone's tier can't run the model; using the tier's");
                fit.clone()
            }
            _ => spec.clone(),
        };
        if !out.contains(&spec) {
            out.push(spec);
        }
    }
    out
}

fn info(entry: &ModelEntry, view: ModelView, root: &Path, vad: Option<&ModelEntry>) -> ModelInfo {
    ModelInfo {
        downloading: Running::token(root, &entry.id).is_some(),
        space_needed: space_needed(entry, root, vad),
        attribution: entry.license.attribution.clone(),
        id: view.id,
        name: view.name,
        spec: view.spec,
        languages: view.languages,
        hinglish: view.hinglish,
        download_bytes: view.download_bytes,
        disk_bytes: view.disk_bytes,
        installed: view.installed,
        recommended: view.recommended,
        license: view.license,
        license_url: view.license_url,
    }
}

/// Free space installing `entry` (and `vad`) needs now: nothing for what's installed.
fn space_needed(entry: &ModelEntry, root: &Path, vad: Option<&ModelEntry>) -> u64 {
    let one = |e: &ModelEntry| {
        if e.is_installed(root) { 0 } else { e.space_needed(bolo_models::partial_bytes(root, e)) }
    };
    one(entry) + vad.filter(|v| v.id != entry.id).map_or(0, one)
}

impl Inner {
    fn vad(&self) -> Option<&ModelEntry> {
        self.manifest.get(VAD_MODEL).ok()
    }

    fn entry(&self, id: &str) -> Result<&ModelEntry, FfiError> {
        self.manifest.get(id).map_err(|e| FfiError::Invalid { detail: e.to_string() })
    }

    /// Tells every `Bolo` on these models.
    fn tell(&self, event: &ModelDownload) {
        for bolo in open_on_models(&self.paths.models_dir) {
            bolo.listener.on_download(event.clone());
        }
    }
}

#[uniffi::export]
impl Bolo {
    /// The tier of a phone with `total_ram_mb` MiB of RAM
    /// (`ActivityManager.MemoryInfo.totalMem / 1048576`).
    pub fn tier(&self, total_ram_mb: u64) -> DeviceTier {
        self.inner.manifest.tier(Platform::Android, total_ram_mb).into()
    }

    /// The models this phone can download (its tier's), the recommended one for the
    /// settings' language first, then by size. The tier comes from `/proc/meminfo`, the
    /// same `MemTotal` that `totalMem` reports.
    pub fn models(&self) -> Vec<ModelInfo> {
        let inner = &self.inner;
        let device = *lock(&inner.device);
        let hinglish = lock(&inner.settings).language != LanguageMode::English;
        let recommended = inner.manifest.recommend(Platform::Android, device.ram_mb(), hinglish).map(|e| e.id.clone());
        let root = &inner.paths.models_dir;
        bolo_models::views(&inner.manifest, device.tier(&inner.manifest), root, recommended.as_deref())
            .into_iter()
            .filter_map(|view| {
                let entry = inner.manifest.get(&view.id).ok()?;
                Some(info(entry, view, root, inner.vad()))
            })
            .collect()
    }

    /// The model to suggest for a phone with `total_ram_mb` MiB of RAM dictating in
    /// `language`: Nemotron on ≥ 5000 MiB; below that, Moonshine-tiny for English and
    /// Whisper-base Hinglish for Hinglish and Auto. `None` when nothing local fits (the
    /// app then suggests a cloud engine).
    pub fn recommend(&self, total_ram_mb: u64, language: Language) -> Option<ModelInfo> {
        let inner = &self.inner;
        let hinglish = language != Language::English;
        let entry = inner.manifest.recommend(Platform::Android, total_ram_mb, hinglish)?;
        let root = &inner.paths.models_dir;
        Some(info(entry, ModelView::of(entry, root, Some(&entry.id)), root, inner.vad()))
    }

    /// Downloads, verifies and installs model `id` (and the VAD, if it isn't installed
    /// yet), resuming a partial download. It blocks until the model is in: call it from
    /// a worker thread, inside the app's foreground service. Progress goes to every
    /// `Bolo` on these models through `on_download`, about 10 times a second.
    ///
    /// `Ok(true)` when installed, `Ok(false)` when cancelled. Refused with
    /// [`FfiError::NoSpace`] before anything downloads when storage is short, and
    /// [`FfiError::Invalid`] for an unknown id or one already downloading.
    pub fn download_model(&self, id: String) -> Result<bool, FfiError> {
        let inner = &self.inner;
        let root = inner.paths.models_dir.clone();
        let entry = inner.entry(&id)?;
        let Some((_running, cancel)) = Running::begin(&root, &id) else {
            return Err(FfiError::Invalid { detail: format!("{id} is already downloading") });
        };
        let vad = inner.vad();
        let fail = |e: FfiError| {
            inner.tell(&ModelDownload::end(entry, DownloadPhase::Failed, Some(e.to_string())));
            e
        };
        std::fs::create_dir_all(&root).map_err(|e| fail(FfiError::Download { detail: e.to_string() }))?;
        let needed = space_needed(entry, &root, vad);
        if let Some(available) = device::free_bytes(&root)
            && available < needed
        {
            return Err(fail(FfiError::NoSpace { needed, available }));
        }
        let total = entry.download.as_ref().map_or(0, |d| d.size_bytes);
        let partial = bolo_models::partial_bytes(&root, entry);
        inner.tell(&ModelDownload {
            id: id.clone(),
            phase: DownloadPhase::Downloading,
            downloaded: partial,
            total,
            error: None,
        });
        // A client per download: its connections belong to this download's runtime.
        let client = bolo_stt::cloud::http_client().map_err(|e| fail(FfiError::Download { detail: e.to_string() }))?;
        let started = Instant::now();
        let downloader = Downloader::new(client, &root);
        let installed = run(&downloader, entry, vad, &cancel, PROGRESS_INTERVAL, &|event| inner.tell(&event))
            .map_err(|e| FfiError::Download { detail: e.to_string() })?;
        if installed {
            tracing::info!(model = %id, secs = started.elapsed().as_secs_f32(), resumed_from = partial, "model downloaded");
            // The next dictation builds its engine with the model there.
            for bolo in open_on_models(&root) {
                bolo.engine.update_config(bolo.config());
            }
        }
        Ok(installed)
    }

    /// Stops model `id`'s download, from any `Bolo` in the process. What's downloaded
    /// so far is kept for the next attempt. Whether one was running.
    pub fn cancel_download(&self, id: String) -> bool {
        let Some(token) = Running::token(&self.inner.paths.models_dir, &id) else { return false };
        token.cancel();
        true
    }

    /// Deletes model `id` and any partial download of it. Refused while it downloads.
    pub fn delete_model(&self, id: String) -> Result<(), FfiError> {
        let inner = &self.inner;
        let root = &inner.paths.models_dir;
        let entry = inner.entry(&id)?;
        if Running::token(root, &id).is_some() {
            return Err(FfiError::Invalid { detail: format!("{id} is downloading; cancel it first") });
        }
        let bolos = open_on_models(root);
        for bolo in &bolos {
            // The engine may hold the model: let it go first.
            bolo.engine.release_resources();
        }
        bolo_models::uninstall(root, entry).map_err(|e| FfiError::Download { detail: e.to_string() })?;
        tracing::info!(model = %id, "model deleted");
        for bolo in &bolos {
            bolo.engine.update_config(bolo.config());
        }
        Ok(())
    }
}

impl Bolo {
    /// Pretend the phone has `total_ram_mb` and `big_cores`, for tests.
    #[doc(hidden)]
    pub fn set_device(&self, total_ram_mb: Option<u64>, big_cores: Option<u32>) {
        *lock(&self.inner.device) = Device { total_ram_mb, big_cores: big_cores.map(|n| n as usize) };
        self.inner.engine.update_config(self.inner.config());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(phase: Phase, downloaded: u64) -> DownloadProgress {
        DownloadProgress { phase, downloaded, total: 100 }
    }

    #[test]
    fn throttle_passes_ten_a_second_and_every_phase_change() {
        let mut t = Throttle::new(Duration::from_millis(100));
        let t0 = Instant::now();
        let at = |ms| t0 + Duration::from_millis(ms);
        assert!(t.pass(&p(Phase::Downloading, 1), at(0)), "the first");
        assert!(!t.pass(&p(Phase::Downloading, 2), at(30)));
        assert!(!t.pass(&p(Phase::Downloading, 3), at(99)));
        assert!(t.pass(&p(Phase::Downloading, 4), at(100)));
        assert!(!t.pass(&p(Phase::Downloading, 5), at(150)));
        assert!(t.pass(&p(Phase::Downloading, 100), at(160)), "the last byte");
        assert!(t.pass(&p(Phase::Verifying, 100), at(161)), "a new phase");
        assert!(t.pass(&p(Phase::Unpacking, 100), at(162)));
        assert!(t.pass(&p(Phase::Unpacking, 100), at(163)), "nothing after downloading is held back");
    }

    #[test]
    fn a_steady_stream_comes_out_at_about_ten_hertz() {
        let mut t = Throttle::new(PROGRESS_INTERVAL);
        let t0 = Instant::now();
        // 2 s of 1 MiB chunks every 5 ms (200 MiB/s).
        let passed = (0..400u64)
            .filter(|&i| {
                t.pass(
                    &DownloadProgress { phase: Phase::Downloading, downloaded: i, total: 1000 },
                    t0 + Duration::from_millis(i * 5),
                )
            })
            .count();
        assert_eq!(passed, 20);
    }

    #[test]
    fn low_tier_auto_runs_the_hinglish_model_not_nemotron() {
        let m = Manifest::bundled().unwrap();
        let low = Device { total_ram_mb: Some(3_700), big_cores: Some(2) };
        let high = Device { total_ram_mb: Some(5_929), big_cores: Some(4) };
        let specs = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let nemotron = specs(&["local:nemotron-stream"]);
        assert_eq!(fit_engines(&nemotron, &m, &low, LanguageMode::Auto), ["local:whisper-base-hinglish"]);
        assert_eq!(fit_engines(&nemotron, &m, &low, LanguageMode::Hinglish), ["local:whisper-base-hinglish"]);
        assert_eq!(fit_engines(&nemotron, &m, &low, LanguageMode::English), ["local:moonshine-tiny"]);
        assert_eq!(fit_engines(&nemotron, &m, &high, LanguageMode::Auto), nemotron, "high tier keeps Nemotron");
        // The tier's own models, cloud engines and unknown ids stay; duplicates go.
        let chain = specs(&["local:parakeet-v3", "deepgram", "local:moonshine-tiny", "local:qwen3-asr", "local:nope"]);
        assert_eq!(
            fit_engines(&chain, &m, &low, LanguageMode::English),
            ["local:moonshine-tiny", "deepgram", "local:nope"]
        );
        let unknown = Device { total_ram_mb: None, big_cores: None };
        assert_eq!(fit_engines(&nemotron, &m, &unknown, LanguageMode::Auto), nemotron);
    }
}
