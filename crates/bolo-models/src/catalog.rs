//! The model list a settings page shows, for any platform: which models a device tier
//! can download, which are installed and which one is recommended. The desktop's model
//! page and the Android app (`bolo-ffi`) both build theirs from [`views`].

use std::path::Path;

use serde::Serialize;

use crate::manifest::{Manifest, ModelEntry, ModelKind, ModelStatus, Tier};

/// The pause detector's model, downloaded alongside the first ASR model (see
/// [`crate::Downloader::install_with_vad`]).
pub const VAD_MODEL: &str = "silero-vad";

/// Headroom on top of the unpacked size that a download needs free (20%).
const SPACE_HEADROOM_PERCENT: u64 = 20;

/// One model as a settings page lists it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ModelView {
    pub id: String,
    pub name: String,
    /// `local:<id>`, the engine spec that uses it.
    pub spec: String,
    pub languages: Vec<String>,
    /// The download, in bytes.
    pub download_bytes: u64,
    /// On disk once unpacked, if the manifest knows.
    pub disk_bytes: Option<u64>,
    pub installed: bool,
    pub recommended: bool,
    /// Speaks Hindi (and so Hinglish).
    pub hinglish: bool,
    /// The licence's SPDX-style id (`MIT`, `Apache-2.0`, `OpenMDW-1.1`, ...).
    pub license: String,
    pub license_url: String,
}

impl ModelView {
    pub fn of(entry: &ModelEntry, root: &Path, recommended: Option<&str>) -> Self {
        Self {
            id: entry.id.clone(),
            name: entry.name.clone(),
            spec: format!("local:{}", entry.id),
            languages: entry.languages.clone(),
            download_bytes: entry.download.as_ref().map_or(0, |d| d.size_bytes),
            disk_bytes: entry.unpacked_size_bytes,
            installed: entry.is_installed(root),
            recommended: recommended == Some(entry.id.as_str()),
            hinglish: entry.speaks_hindi(),
            license: entry.license.id.clone(),
            license_url: entry.license.url.clone(),
        }
    }
}

/// The ASR models a device of `tier` can download, recommended first, then by size.
pub fn views(manifest: &Manifest, tier: Tier, root: &Path, recommended: Option<&str>) -> Vec<ModelView> {
    let mut v: Vec<ModelView> = manifest
        .models
        .iter()
        .filter(|m| m.kind == ModelKind::Asr && m.status == ModelStatus::Available && m.tiers.contains(&tier))
        .map(|m| ModelView::of(m, root, recommended))
        .collect();
    v.sort_by_key(|m| (!m.recommended, m.download_bytes));
    v
}

impl ModelEntry {
    /// Speaks Hindi (and so Hinglish).
    pub fn speaks_hindi(&self) -> bool {
        self.languages.iter().any(|l| l == "hi")
    }

    /// The free space installing this model needs, with `partial` bytes of the archive
    /// already downloaded: the rest of the archive, plus the unpacked model with 20%
    /// headroom (the archive and the unpacked files are both on disk until the archive is
    /// deleted). An unknown unpacked size counts as twice the archive.
    pub fn space_needed(&self, partial: u64) -> u64 {
        let archive = self.download.as_ref().map_or(0, |d| d.size_bytes);
        let unpacked = self.unpacked_size_bytes.unwrap_or(archive.saturating_mul(2));
        archive.saturating_sub(partial) + unpacked + unpacked * SPACE_HEADROOM_PERCENT / 100
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::Platform;

    #[test]
    fn each_tier_lists_its_own_models() {
        let m = Manifest::bundled().unwrap();
        let ids = |tier| views(&m, tier, Path::new("no-such-dir"), None).into_iter().map(|v| v.id).collect::<Vec<_>>();
        assert_eq!(ids(Tier::AndroidLow), ["moonshine-tiny", "whisper-base-hinglish"]);
        assert_eq!(ids(Tier::AndroidHigh), ["moonshine-tiny", "nemotron-stream"]);
        assert!(ids(Tier::Desktop).len() > 4);
        for tier in [Tier::Desktop, Tier::AndroidHigh, Tier::AndroidLow] {
            assert!(!ids(tier).iter().any(|id| id == VAD_MODEL), "never the VAD");
        }
    }

    #[test]
    fn the_recommended_model_comes_first_with_its_licence() {
        let m = Manifest::bundled().unwrap();
        let rec = m.recommend(Platform::Android, 3_600, true).map(|e| e.id.clone());
        let v = views(&m, Tier::AndroidLow, Path::new("no-such-dir"), rec.as_deref());
        assert_eq!(v[0].id, "whisper-base-hinglish");
        assert!(v[0].recommended && v[0].hinglish && !v[0].installed);
        assert_eq!(v[0].license, "Apache-2.0");
        assert_eq!(v[0].spec, "local:whisper-base-hinglish");
        assert_eq!(v[1].license, "MIT");
    }

    #[test]
    fn space_needed_is_the_rest_of_the_archive_plus_the_unpacked_model_and_a_fifth() {
        let m = Manifest::bundled().unwrap();
        let moonshine = m.get("moonshine-tiny").unwrap();
        let (archive, unpacked) = (29_858_559, 44_243_206);
        assert_eq!(moonshine.space_needed(0), archive + unpacked + unpacked / 5);
        assert_eq!(moonshine.space_needed(archive), unpacked + unpacked / 5, "fully downloaded");
        assert_eq!(moonshine.space_needed(archive + 5), unpacked + unpacked / 5, "never negative");
        let parakeet = m.get("parakeet-v2-en").unwrap();
        assert!(parakeet.unpacked_size_bytes.is_none());
        let archive = parakeet.download.as_ref().unwrap().size_bytes;
        assert_eq!(parakeet.space_needed(0), archive + 2 * archive + 2 * archive / 5);
    }
}
