//! Local models for the settings window and onboarding: which ones this PC can run,
//! which are installed, the recommended one, and downloads with progress.
//!
//! Downloads go through [`bolo_models::Downloader`]: resumable (a cancelled or broken
//! download keeps its `.part` file and picks up from there), SHA-256 verified, unpacked
//! atomically. The Silero VAD model, which per-segment polish uses to find pauses, comes
//! along with the first ASR model.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use bolo_models::{DownloadProgress, Downloader, Manifest, ModelEntry, Phase, Platform, Tier};
pub use bolo_models::{ModelView, VAD_MODEL};

/// The ASR models a Windows PC can download, recommended first, then by size.
pub fn views(manifest: &Manifest, root: &Path, recommended: Option<&str>) -> Vec<ModelView> {
    bolo_models::views(manifest, Tier::Desktop, root, recommended)
}

/// The model to suggest for a PC with `total_ram_mb` of memory.
pub fn recommend(manifest: &Manifest, total_ram_mb: u64, hinglish: bool) -> Option<&ModelEntry> {
    manifest.recommend(Platform::Desktop, total_ram_mb, hinglish)
}

/// This PC's memory in MB (0 if it can't be read).
pub fn total_ram_mb() -> u64 {
    let Ok(mut sys) = sysinfo::System::new() else { return 0 };
    sys.refresh_memory();
    sys.total_memory() / (1024 * 1024)
}

/// A download's progress, sent to the webviews as it goes.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DownloadEvent {
    pub id: String,
    /// `downloading`, `verifying`, `unpacking`, `done`, `failed` or `cancelled`.
    pub phase: &'static str,
    pub downloaded: u64,
    pub total: u64,
    pub error: Option<String>,
}

impl DownloadEvent {
    fn progress(id: &str, p: DownloadProgress) -> Self {
        let phase = match p.phase {
            Phase::Downloading => "downloading",
            Phase::Verifying => "verifying",
            Phase::Unpacking => "unpacking",
            Phase::Done => "done",
        };
        Self { id: id.to_owned(), phase, downloaded: p.downloaded, total: p.total, error: None }
    }

    fn end(id: &str, phase: &'static str, error: Option<String>) -> Self {
        Self { id: id.to_owned(), phase, downloaded: 0, total: 0, error }
    }
}

/// Where download progress goes.
pub type Report = Arc<dyn Fn(DownloadEvent) + Send + Sync>;

/// The downloads in flight, one per model.
#[derive(Default)]
pub struct Downloads {
    tasks: Arc<Mutex<HashMap<String, tauri::async_runtime::JoinHandle<()>>>>,
}

impl Downloads {
    /// Download `id` into `root`, unless it's already downloading. Progress, then `done`
    /// or `failed`, go to `report`.
    pub fn start(&self, manifest: &Manifest, root: PathBuf, id: &str, report: Report) -> Result<(), String> {
        let entry = manifest.get(id).map_err(|e| e.to_string())?.clone();
        let vad = manifest.get(VAD_MODEL).ok().cloned();
        let mut tasks = self.tasks.lock().unwrap_or_else(PoisonError::into_inner);
        if tasks.get(id).is_some_and(|t| !t.inner().is_finished()) {
            return Ok(());
        }
        let client = bolo_stt::cloud::http_client().map_err(|e| e.to_string())?;
        let all = self.tasks.clone();
        let key = id.to_owned();
        let task = tauri::async_runtime::spawn(async move {
            let id = entry.id.clone();
            let dl = Downloader::new(client, root);
            let progress = |p: DownloadProgress| {
                // `done` is sent once the VAD is in too.
                if p.phase != Phase::Done {
                    report(DownloadEvent::progress(&id, p));
                }
            };
            match dl.install_with_vad(&entry, vad.as_ref(), &progress).await {
                Ok(_) => report(DownloadEvent::end(&id, "done", None)),
                Err(e) => {
                    tracing::warn!(model = %id, %e, "model download failed");
                    report(DownloadEvent::end(&id, "failed", Some(e.to_string())));
                }
            }
            all.lock().unwrap_or_else(PoisonError::into_inner).remove(&id);
        });
        tasks.insert(key, task);
        Ok(())
    }

    /// Stop downloading `id`. What's downloaded so far is kept for the next start.
    pub fn cancel(&self, id: &str, report: &Report) {
        if let Some(task) = self.tasks.lock().unwrap_or_else(PoisonError::into_inner).remove(id) {
            task.abort();
            report(DownloadEvent::end(id, "cancelled", None));
        }
    }

    pub fn running(&self) -> Vec<String> {
        self.tasks.lock().unwrap_or_else(PoisonError::into_inner).keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use bolo_models::Manifest;

    use super::{recommend, views};

    #[test]
    fn the_list_has_desktop_asr_models_recommended_first() {
        let m = Manifest::bundled().unwrap();
        let rec = recommend(&m, 16_000, true).map(|e| e.id.clone());
        assert_eq!(rec.as_deref(), Some("nemotron-stream"));
        let v = views(&m, Path::new("no-such-dir"), rec.as_deref());
        assert_eq!(v[0].id, "nemotron-stream");
        assert!(v[0].recommended && v[0].hinglish);
        assert_eq!(v[0].spec, "local:nemotron-stream");
        assert!(v.iter().all(|m| !m.installed && m.download_bytes > 0));
        assert!(v.iter().all(|m| m.id != "silero-vad"), "no VAD");
        assert!(v.iter().all(|m| m.id != "qwen3-asr-hinglish"), "nothing that needs an export");
        let rest: Vec<u64> = v[1..].iter().map(|m| m.download_bytes).collect();
        assert!(rest.is_sorted(), "then smallest first");
    }

    #[test]
    fn a_small_pc_gets_a_smaller_model_or_none() {
        let m = Manifest::bundled().unwrap();
        assert_eq!(recommend(&m, 1_000, false).map(|e| e.id.as_str()), Some("moonshine-tiny"));
        assert_eq!(recommend(&m, 1_000, true).map(|e| e.id.as_str()), None);
    }
}
