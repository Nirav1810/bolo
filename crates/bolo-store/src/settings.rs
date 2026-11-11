//! `settings.toml`: what the user chose, versioned so later releases can migrate it.
//!
//! - The file starts with `version = 1`. A file without `version` is version 0, the flat
//!   layout of the `BOLO_*` variables the desktop shell read before this crate (ticket
//!   01): `polish = "off" | "on" | "<tone>"`, `history = <bool>`, `history_days = <n>`.
//!   [`Settings::load`] migrates older versions step by step and saves the result.
//! - Keys this version doesn't know are kept through a load and save, so a newer Bolo's
//!   settings survive a run of an older one.
//! - A file that doesn't parse, or whose values have the wrong types, is renamed to
//!   `settings.toml.bad-<unix seconds>` and replaced with the defaults.
//! - A file from a newer Bolo ([`StoreError::NewerSettings`]) is left alone.
//!
//! API keys never live here: see [`crate::secrets`].

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use bolo_core::{ANTHROPIC, ApiKeys, EngineConfig, HinglishScript, LanguageMode, Polish};
use serde::{Deserialize, Serialize};
use toml::{Table, Value};

use crate::{Result, Secrets, StoreError, io_err};

/// The settings layout this build writes.
pub const SETTINGS_VERSION: i64 = 1;

type Migration = fn(&mut Table) -> Result<(), String>;

/// One step per old version: `MIGRATIONS[v]` turns a version-`v` table into `v + 1`.
const MIGRATIONS: [Migration; SETTINGS_VERSION as usize] = [v0_to_v1];

/// The key held to dictate (ADR-0017). Caps Lock by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PushToTalkKey {
    #[default]
    CapsLock,
    RightCtrl,
    RightAlt,
    F13,
    F14,
    F15,
    F16,
    F17,
    F18,
    F19,
    F20,
    F21,
    F22,
    F23,
    F24,
}

/// Polish (ADR-0016): on or off, the tone, and per-app overrides.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PolishSettings {
    pub enabled: bool,
    /// "neutral", "casual" or "formal".
    pub tone: String,
    /// Lower-case exe name → polish on/off there. Wins over the bundled app rules.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub apps: BTreeMap<String, bool>,
    #[serde(flatten)]
    pub unknown: Table,
}

impl Default for PolishSettings {
    fn default() -> Self {
        Self { enabled: false, tone: "neutral".into(), apps: BTreeMap::new(), unknown: Table::new() }
    }
}

/// Dictation history: text only, pruned after `retention_days`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HistorySettings {
    /// Off stops new rows; it doesn't delete the old ones.
    pub enabled: bool,
    pub retention_days: u32,
    #[serde(flatten)]
    pub unknown: Table,
}

impl Default for HistorySettings {
    fn default() -> Self {
        Self { enabled: true, retention_days: 30, unknown: Table::new() }
    }
}

/// Which builds the app updates to (ADR-0019).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UpdateChannel {
    /// The `vX.Y.Z` releases.
    Beta,
    /// A build of every push to `main`.
    Nightly,
}

/// Self-update (ADR-0019).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdateSettings {
    /// `None` until the user picks one: the build's own channel then applies (a nightly
    /// follows Nightly, a release Beta).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel: Option<UpdateChannel>,
    #[serde(flatten)]
    pub unknown: Table,
}

/// The overlay pill's always-on idle button (M3 ticket 15).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OverlaySettings {
    /// Keep the pill on screen as a small button while Bolo is idle.
    pub idle_button: bool,
    /// Where the pill sits along the bottom of the work area: 0 left, 0.5 centre, 1 right.
    pub position: f64,
    #[serde(flatten)]
    pub unknown: Table,
}

impl Default for OverlaySettings {
    fn default() -> Self {
        Self { idle_button: true, position: 0.5, unknown: Table::new() }
    }
}

/// Everything in `settings.toml`. Defaults match a fresh install.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Engine specs, primary first: `local:<model-id>`, `deepgram`, `groq[:model]`, ...
    pub engines: Vec<String>,
    pub language: LanguageMode,
    pub script: HinglishScript,
    /// Names and terms to spell right; fed to the engines and to polish.
    pub dictionary: Vec<String>,
    pub push_to_talk: PushToTalkKey,
    /// Start with Windows. Off by default.
    pub autostart: bool,
    /// Overrides the ADR-007 models directory.
    pub models_dir: Option<PathBuf>,
    pub polish: PolishSettings,
    pub history: HistorySettings,
    pub overlay: OverlaySettings,
    pub updates: UpdateSettings,
    /// First-run onboarding's step while it's unfinished (0 = welcome); `None` once it's
    /// done. A settings file from before onboarding existed has none, so it isn't shown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub onboarding_step: Option<u32>,
    /// Keys this version doesn't know, written back as they were.
    #[serde(flatten)]
    pub unknown: Table,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            engines: EngineConfig::default().engines,
            language: LanguageMode::default(),
            script: HinglishScript::default(),
            dictionary: Vec::new(),
            push_to_talk: PushToTalkKey::default(),
            autostart: false,
            models_dir: None,
            polish: PolishSettings::default(),
            history: HistorySettings::default(),
            overlay: OverlaySettings::default(),
            updates: UpdateSettings::default(),
            onboarding_step: None,
            unknown: Table::new(),
        }
    }
}

/// The result of [`Settings::load`].
#[derive(Debug)]
pub struct Loaded {
    pub settings: Settings,
    /// The file was bad: it was moved here and the defaults were saved in its place.
    pub recovered_from: Option<PathBuf>,
    /// The file was an older version, migrated and saved.
    pub migrated_from: Option<i64>,
    /// There was no file: Bolo's first run.
    pub fresh: bool,
}

impl Settings {
    /// `%APPDATA%\Bolo\settings.toml`.
    pub fn default_path() -> Result<PathBuf> {
        Ok(crate::config_dir()?.join("settings.toml"))
    }

    /// Reads `path`. A missing file gives the defaults and writes nothing. See the module
    /// docs for migration and bad-file recovery.
    pub fn load(path: &Path) -> Result<Loaded> {
        let text = match fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Loaded {
                    settings: Self::default(),
                    recovered_from: None,
                    migrated_from: None,
                    fresh: true,
                });
            }
            Err(e) => return Err(io_err(path)(e)),
        };
        match Self::parse(&text) {
            Ok((settings, version)) => {
                let migrated_from = (version < SETTINGS_VERSION).then_some(version);
                if migrated_from.is_some() {
                    settings.save(path)?;
                }
                Ok(Loaded { settings, recovered_from: None, migrated_from, fresh: false })
            }
            Err(Bad::Newer(v)) => Err(StoreError::NewerSettings(v)),
            Err(Bad::Invalid(why)) => {
                let backup = backup_path(path);
                tracing::warn!(file = %path.display(), backup = %backup.display(), %why, "bad settings; using the defaults");
                fs::rename(path, &backup).map_err(io_err(path))?;
                let settings = Self::default();
                settings.save(path)?;
                Ok(Loaded { settings, recovered_from: Some(backup), migrated_from: None, fresh: false })
            }
        }
    }

    /// The settings and the file's version, migrated to [`SETTINGS_VERSION`].
    fn parse(text: &str) -> Result<(Self, i64), Bad> {
        let mut table: Table = text.parse().map_err(|e: toml::de::Error| Bad::Invalid(e.to_string()))?;
        let version = match table.remove("version") {
            None => 0,
            Some(Value::Integer(v)) if v >= 0 => v,
            Some(v) => return Err(Bad::Invalid(format!("version = {v} is not a version number"))),
        };
        if version > SETTINGS_VERSION {
            return Err(Bad::Newer(version));
        }
        for step in &MIGRATIONS[version as usize..] {
            step(&mut table).map_err(Bad::Invalid)?;
        }
        let settings = Self::deserialize(table).map_err(|e| Bad::Invalid(e.to_string()))?;
        Ok((settings, version))
    }

    /// Writes `path` (and its directory) through a temp file, so a crash never leaves a
    /// half-written file.
    pub fn save(&self, path: &Path) -> Result<()> {
        // Through a `Table` so plain values come before tables; `version` leads the file.
        let text = format!(
            "version = {SETTINGS_VERSION}
{}",
            toml::to_string(&Table::try_from(self)?)?
        );
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(io_err(dir))?;
        }
        let tmp = path.with_extension("toml.tmp");
        fs::write(&tmp, text).map_err(io_err(&tmp))?;
        fs::rename(&tmp, path).map_err(io_err(path))
    }

    /// The dictionary, trimmed, without blanks or repeats (first spelling wins).
    pub fn dictionary_terms(&self) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        self.dictionary
            .iter()
            .map(|t| t.trim())
            .filter(|t| !t.is_empty() && seen.insert(t.to_lowercase()))
            .map(String::from)
            .collect()
    }

    /// The engine config for these settings and keys. The dictionary feeds both the
    /// engines and polish. Polish is on only when it's enabled *and* the Anthropic key
    /// is stored; the app shows the missing key, the engine just runs without polish.
    /// Polish's `app` is left empty: the app fills it per dictation from the target.
    pub fn engine_config(&self, secrets: &Secrets) -> EngineConfig {
        self.engine_config_with_keys(secrets.api_keys().clone())
    }

    /// [`Self::engine_config`] with the keys given directly (say, none, when the
    /// credential store can't be opened).
    pub fn engine_config_with_keys(&self, api_keys: ApiKeys) -> EngineConfig {
        let defaults = EngineConfig::default();
        let engines: Vec<String> =
            self.engines.iter().map(|e| e.trim()).filter(|e| !e.is_empty()).map(String::from).collect();
        let dictionary = self.dictionary_terms();
        let polish = if self.polish.enabled && api_keys.get(ANTHROPIC).is_some() {
            let tone = self.polish.tone.trim();
            let tone = if tone.is_empty() { "neutral" } else { tone };
            Polish::On { tone: tone.into(), app: String::new(), dictionary: dictionary.clone() }
        } else {
            Polish::Off
        };
        EngineConfig {
            engines: if engines.is_empty() { defaults.engines } else { engines },
            language: self.language,
            script: self.script,
            dictionary,
            models_dir: self.models_dir.clone().or_else(bolo_models::default_models_root).unwrap_or_default(),
            polish,
            api_keys,
            ..defaults
        }
    }
}

enum Bad {
    Newer(i64),
    Invalid(String),
}

/// `settings.toml.bad-<unix seconds>`, next to the file.
fn backup_path(path: &Path) -> PathBuf {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".bad-{secs}"));
    path.with_file_name(name)
}

/// v0, the flat `BOLO_*` layout → v1: `polish` and `history` become tables.
fn v0_to_v1(t: &mut Table) -> Result<(), String> {
    match t.remove("polish") {
        None => {}
        Some(Value::String(s)) => {
            let s = s.trim().to_lowercase();
            let mut polish = Table::new();
            polish.insert("enabled".into(), Value::Boolean(!s.is_empty() && s != "off"));
            let tone = if matches!(s.as_str(), "" | "on" | "off") { "neutral" } else { s.as_str() };
            polish.insert("tone".into(), Value::String(tone.into()));
            t.insert("polish".into(), Value::Table(polish));
        }
        Some(v) => return Err(format!("v0 polish = {v}: expected a string")),
    }
    let mut history = Table::new();
    if let Some(on) = t.remove("history") {
        history.insert("enabled".into(), on);
    }
    if let Some(days) = t.remove("history_days") {
        history.insert("retention_days".into(), days);
    }
    if !history.is_empty() {
        t.insert("history".into(), Value::Table(history));
    }
    Ok(())
}

#[cfg(test)]
mod tests {

    use bolo_core::{ANTHROPIC, HinglishScript, LanguageMode, Polish};

    use super::*;

    fn secrets(keys: &[(&str, &str)]) -> Secrets {
        let mut s = Secrets::with_store(keyring_core::mock::Store::new().unwrap()).unwrap();
        for (p, k) in keys {
            s.set(p, k).unwrap();
        }
        s
    }

    fn custom() -> Settings {
        let mut s = Settings {
            engines: vec!["deepgram-stream".into(), "local:nemotron-stream".into()],
            language: LanguageMode::Hinglish,
            script: HinglishScript::Devanagari,
            dictionary: vec!["Nirav".into(), "Bolo".into()],
            push_to_talk: PushToTalkKey::F13,
            autostart: true,
            models_dir: Some(PathBuf::from(r"D:\models")),
            ..Default::default()
        };
        s.polish.enabled = true;
        s.polish.tone = "casual".into();
        s.polish.apps.insert("slack.exe".into(), false);
        s.history.retention_days = 7;
        s.overlay.idle_button = false;
        s.overlay.position = 0.25;
        s.updates.channel = Some(UpdateChannel::Nightly);
        s
    }

    #[test]
    fn round_trip_keeps_every_field() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Bolo").join("settings.toml");
        let s = custom();
        s.save(&path).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("version = 1\n"), "{text}");
        let loaded = Settings::load(&path).unwrap();
        assert_eq!(loaded.settings, s);
        assert!(loaded.recovered_from.is_none() && loaded.migrated_from.is_none());
    }

    #[test]
    fn the_onboarding_step_round_trips_and_is_absent_when_done() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        let s = Settings { onboarding_step: Some(2), ..Settings::default() };
        s.save(&path).unwrap();
        assert_eq!(Settings::load(&path).unwrap().settings.onboarding_step, Some(2));
        Settings::default().save(&path).unwrap();
        assert!(!fs::read_to_string(&path).unwrap().contains("onboarding"));
        assert_eq!(Settings::load(&path).unwrap().settings.onboarding_step, None);
    }

    #[test]
    fn the_update_channel_is_absent_until_chosen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        Settings::default().save(&path).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(!text.contains("channel"), "{text}");
        fs::write(&path, "version = 1\n\n[updates]\nchannel = \"nightly\"\n").unwrap();
        assert_eq!(Settings::load(&path).unwrap().settings.updates.channel, Some(UpdateChannel::Nightly));
    }

    #[test]
    fn a_missing_file_gives_the_defaults_and_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        let loaded = Settings::load(&path).unwrap();
        assert_eq!(loaded.settings, Settings::default());
        assert!(loaded.fresh);
        assert!(!path.exists());
        Settings::default().save(&path).unwrap();
        assert!(!Settings::load(&path).unwrap().fresh);
        let d = Settings::default();
        assert!(d.history.enabled && d.history.retention_days == 30);
        assert!(!d.polish.enabled && !d.autostart);
        assert_eq!(d.push_to_talk, PushToTalkKey::CapsLock);
    }

    #[test]
    fn unknown_keys_survive_a_load_and_save() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        fs::write(
            &path,
            "version = 1\nfuture_flag = true\nlanguage = \"english\"\n\n[polish]\nenabled = true\nstyle = \"terse\"\n\n\
             [history]\nkeep_audio = false\n\n[sync]\nendpoint = \"top\"\n",
        )
        .unwrap();
        let s = Settings::load(&path).unwrap().settings;
        assert_eq!(s.language, LanguageMode::English);
        s.save(&path).unwrap();
        let t: Table = fs::read_to_string(&path).unwrap().parse().unwrap();
        assert_eq!(t["future_flag"].as_bool(), Some(true));
        assert_eq!(t["polish"]["style"].as_str(), Some("terse"));
        assert_eq!(t["history"]["keep_audio"].as_bool(), Some(false));
        assert_eq!(t["sync"]["endpoint"].as_str(), Some("top"));
        assert_eq!(Settings::load(&path).unwrap().settings, s);
    }

    #[test]
    fn a_v0_file_is_migrated_and_saved_as_v1() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        fs::write(&path, include_str!("../tests/fixtures/settings-v0.toml")).unwrap();
        let loaded = Settings::load(&path).unwrap();
        assert_eq!(loaded.migrated_from, Some(0));
        let s = loaded.settings;
        assert_eq!(s.engines, ["local:parakeet-v2-en", "deepgram"]);
        assert_eq!(s.language, LanguageMode::Hinglish);
        assert_eq!(s.dictionary, ["Nirav", "Bolo"]);
        assert!(s.polish.enabled);
        assert_eq!(s.polish.tone, "casual");
        assert!(!s.history.enabled);
        assert_eq!(s.history.retention_days, 14);
        assert!(s.unknown.is_empty(), "{:?}", s.unknown);
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("version = 1\n"), "{text}");
        let again = Settings::load(&path).unwrap();
        assert_eq!(again.migrated_from, None);
        assert_eq!(again.settings, s);
    }

    #[test]
    fn v0_polish_strings_map_to_on_off_and_a_tone() {
        for (v0, enabled, tone) in [("off", false, "neutral"), ("on", true, "neutral"), ("Formal", true, "formal")] {
            let (s, _) = Settings::parse(&format!("polish = \"{v0}\"")).ok().unwrap();
            assert_eq!((s.polish.enabled, s.polish.tone.as_str()), (enabled, tone), "{v0}");
        }
    }

    #[test]
    fn a_bad_file_is_backed_up_and_replaced_with_the_defaults() {
        for bad in ["version = 1\nlanguage = [unclosed", "version = 1\nlanguage = \"klingon\"", "version = \"one\""] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("settings.toml");
            fs::write(&path, bad).unwrap();
            let loaded = Settings::load(&path).unwrap();
            assert_eq!(loaded.settings, Settings::default(), "{bad}");
            let backup = loaded.recovered_from.expect(bad);
            assert!(backup.file_name().unwrap().to_str().unwrap().starts_with("settings.toml.bad-"));
            assert_eq!(fs::read_to_string(&backup).unwrap(), bad);
            assert_eq!(Settings::load(&path).unwrap().settings, Settings::default());
        }
    }

    #[test]
    fn a_newer_file_is_refused_and_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        fs::write(&path, "version = 9\n").unwrap();
        assert!(matches!(Settings::load(&path), Err(StoreError::NewerSettings(9))));
        assert_eq!(fs::read_to_string(&path).unwrap(), "version = 9\n");
    }

    #[test]
    fn engine_config_maps_settings_keys_and_the_dictionary() {
        let mut s = custom();
        s.dictionary = vec![" Nirav ".into(), "".into(), "nirav".into(), "Bolo".into()];
        let c = s.engine_config(&secrets(&[(ANTHROPIC, "sk-ant"), ("deepgram", "dg")]));
        assert_eq!(c.engines, s.engines);
        assert_eq!(c.language, LanguageMode::Hinglish);
        assert_eq!(c.script, HinglishScript::Devanagari);
        assert_eq!(c.dictionary, ["Nirav", "Bolo"]);
        assert_eq!(c.models_dir, PathBuf::from(r"D:\models"));
        assert_eq!(c.api_keys.get("deepgram"), Some("dg"));
        assert_eq!(
            c.polish,
            Polish::On { tone: "casual".into(), app: String::new(), dictionary: vec!["Nirav".into(), "Bolo".into()] }
        );
    }

    #[test]
    fn engine_config_without_the_anthropic_key_or_engines_falls_back() {
        let mut s = custom();
        s.engines = vec![" ".into()];
        s.models_dir = None;
        let c = s.engine_config(&secrets(&[]));
        assert_eq!(c.polish, Polish::Off);
        assert_eq!(c.engines, EngineConfig::default().engines);
        assert_eq!(Some(c.models_dir), bolo_models::default_models_root());
    }
}
