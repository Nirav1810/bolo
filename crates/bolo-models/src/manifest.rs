use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{ModelError, Result};

/// The manifest shipped with this build.
pub const BUNDLED_MANIFEST: &str = include_str!("../../../models/manifest.json");
pub const SUPPORTED_SCHEMA: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub schema_version: u32,
    pub manifest_version: String,
    pub sherpa_onnx_version: String,
    pub models: Vec<ModelEntry>,
    pub defaults: BTreeMap<String, String>,
    pub tier_thresholds_mb: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelEntry {
    pub id: String,
    pub name: String,
    pub kind: ModelKind,
    pub family: Family,
    pub streaming: bool,
    pub languages: Vec<String>,
    /// How Hindi words are written in the output: `roman`, `devanagari`, `mixed`, or null.
    pub hindi_script: Option<String>,
    pub status: ModelStatus,
    pub min_sherpa_onnx: String,
    pub license: License,
    pub download: Option<Download>,
    /// Role → path relative to the model dir (e.g. `encoder` → `encoder.int8.onnx`).
    pub files: BTreeMap<String, String>,
    #[serde(default)]
    pub options: BTreeMap<String, String>,
    pub unpacked_size_bytes: Option<u64>,
    pub min_ram_mb: u64,
    pub tiers: Vec<Tier>,
    #[serde(default)]
    pub notes: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelKind {
    Asr,
    Vad,
}

/// Which sherpa-onnx config the model loads with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Family {
    OnlineTransducer,
    OfflineNemoTransducer,
    OfflineQwen3Asr,
    OfflineMoonshineV2,
    OfflineWhisper,
    SileroVad,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModelStatus {
    Available,
    /// No upstream sherpa-onnx export yet; we must export and host it ourselves.
    NeedsExport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Tier {
    Desktop,
    AndroidHigh,
    AndroidLow,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct License {
    pub id: String,
    pub redistributable: bool,
    pub commercial: bool,
    pub url: String,
    pub attribution: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Download {
    pub url: String,
    pub sha256: String,
    pub size_bytes: u64,
    /// `tar.bz2` or null for a single file.
    pub archive: Option<String>,
    /// Top-level directory inside the archive.
    pub root_dir: Option<String>,
}

impl Manifest {
    pub fn bundled() -> Result<Self> {
        Self::parse(BUNDLED_MANIFEST)
    }

    pub fn parse(json: &str) -> Result<Self> {
        let m: Manifest = serde_json::from_str(json).map_err(|e| ModelError::Manifest(e.to_string()))?;
        m.validate()?;
        Ok(m)
    }

    pub fn get(&self, id: &str) -> Result<&ModelEntry> {
        self.models.iter().find(|m| m.id == id).ok_or_else(|| ModelError::UnknownModel(id.to_owned()))
    }

    fn validate(&self) -> Result<()> {
        if self.schema_version != SUPPORTED_SCHEMA {
            return Err(ModelError::Manifest(format!("unsupported schema_version {}", self.schema_version)));
        }
        let mut seen = std::collections::HashSet::new();
        for m in &self.models {
            let bad = |why: &str| Err(ModelError::Manifest(format!("{}: {why}", m.id)));
            if !seen.insert(m.id.as_str()) {
                return bad("duplicate id");
            }
            if m.status == ModelStatus::Available && m.download.is_none() {
                return bad("available model without download");
            }
            if let Some(d) = &m.download {
                if d.sha256.len() != 64 || !d.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
                    return bad("sha256 must be 64 hex chars");
                }
                if !d.url.starts_with("https://") {
                    return bad("download url must be https");
                }
                if d.archive.is_some() != d.root_dir.is_some() {
                    return bad("archive and root_dir must be set together");
                }
            }
            if !m.license.redistributable && m.download.as_ref().is_some_and(|d| !d.url.contains("huggingface.co")) {
                return bad("non-redistributable model must download from upstream");
            }
            if m.files.values().any(|p| p.contains("..") || Path::new(p).is_absolute()) {
                return bad("file paths must be relative");
            }
        }
        for id in self.defaults.values() {
            self.get(id)?;
        }
        Ok(())
    }

    /// Device tier from platform and total RAM (`GlobalMemoryStatusEx` on Windows,
    /// `ActivityManager.MemoryInfo.totalMem` on Android).
    pub fn tier(&self, platform: Platform, total_ram_mb: u64) -> Tier {
        match platform {
            Platform::Desktop => Tier::Desktop,
            Platform::Android => {
                let high = self.tier_thresholds_mb.get("android-high").copied().unwrap_or(6000);
                if total_ram_mb >= high { Tier::AndroidHigh } else { Tier::AndroidLow }
            }
        }
    }

    /// Pick the default model for this device, or `None` if nothing local fits
    /// (the app then suggests a BYOK cloud provider).
    pub fn recommend(&self, platform: Platform, total_ram_mb: u64, hinglish: bool) -> Option<&ModelEntry> {
        let tier = self.tier(platform, total_ram_mb);
        let key = match (tier, hinglish) {
            (Tier::Desktop, _) => "desktop",
            (Tier::AndroidHigh, _) => "android-high",
            (Tier::AndroidLow, true) => "android-low-hinglish",
            (Tier::AndroidLow, false) => "android-low",
        };
        // Never use more than half of RAM for the model.
        let usable = |m: &&ModelEntry| {
            m.kind == ModelKind::Asr
                && m.status == ModelStatus::Available
                && m.min_ram_mb <= total_ram_mb / 2
                && m.tiers.contains(&tier)
                && (!hinglish || m.languages.iter().any(|l| l == "hi"))
        };
        self.defaults
            .get(key)
            .and_then(|id| self.get(id).ok())
            .filter(usable)
            .or_else(|| self.models.iter().find(usable))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Desktop,
    Android,
}

impl ModelEntry {
    /// Directory the model lives in once installed under `models_root`.
    pub fn install_dir(&self, models_root: &Path) -> PathBuf {
        models_root.join(&self.id)
    }

    /// Absolute path of a file role, e.g. `encoder`.
    pub fn file(&self, models_root: &Path, role: &str) -> Option<PathBuf> {
        self.files.get(role).map(|p| self.install_dir(models_root).join(p))
    }

    pub fn is_installed(&self, models_root: &Path) -> bool {
        let dir = self.install_dir(models_root);
        dir.join(crate::download::INSTALLED_MARKER).is_file() && self.files.values().all(|p| dir.join(p).exists())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_manifest_is_valid() {
        let m = Manifest::bundled().unwrap();
        assert!(m.get("nemotron-stream").unwrap().streaming);
        assert_eq!(m.get("parakeet-v3").unwrap().family, Family::OfflineNemoTransducer);
        assert!(m.models.iter().all(|x| x.license.redistributable));
    }

    #[test]
    fn recommends_by_ram() {
        let m = Manifest::bundled().unwrap();
        assert_eq!(m.recommend(Platform::Desktop, 16_000, true).unwrap().id, "nemotron-stream");
        assert_eq!(m.recommend(Platform::Android, 8_000, false).unwrap().id, "nemotron-stream");
        assert_eq!(m.recommend(Platform::Android, 4_000, false).unwrap().id, "moonshine-tiny");
        // whisper-base-hinglish needs export; until then a 4 GB phone wanting Hinglish
        // gets nothing local (cloud only) rather than an English-only model.
        assert!(m.recommend(Platform::Android, 4_000, true).is_none());
    }

    #[test]
    fn rejects_bad_hash() {
        let mut v: serde_json::Value = serde_json::from_str(BUNDLED_MANIFEST).unwrap();
        v["models"][0]["download"]["sha256"] = "abc".into();
        assert!(Manifest::parse(&v.to_string()).is_err());
    }
}
