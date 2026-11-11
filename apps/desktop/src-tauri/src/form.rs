//! The settings window's form: the part of [`Settings`] the window edits, and what a
//! change to it means for the running app.
//!
//! The window sends the whole [`Form`] on every change (Windows Settings style: no Save
//! button). [`Form::apply`] tidies it into the stored settings, keeping everything the
//! form doesn't cover (the overlay, the models directory, unknown keys), and [`changes`]
//! says which parts of the app have to follow. Both are pure, so they're tested here.

use std::collections::BTreeMap;

use bolo_core::{HinglishScript, LanguageMode};
use bolo_store::{PushToTalkKey, Settings};
use serde::{Deserialize, Serialize};

/// The tones polish knows.
pub const TONES: [&str; 3] = ["neutral", "casual", "formal"];

/// The history retention the window offers, in days.
pub const RETENTION_DAYS: [u32; 5] = [1, 7, 30, 90, 365];

/// What the settings window edits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Form {
    pub push_to_talk: PushToTalkKey,
    pub autostart: bool,
    pub language: LanguageMode,
    pub script: HinglishScript,
    /// Engine specs, primary first.
    pub engines: Vec<String>,
    pub dictionary: Vec<String>,
    pub polish: bool,
    pub tone: String,
    /// Exe → polish on/off there.
    pub polish_apps: BTreeMap<String, bool>,
    pub history: bool,
    pub retention_days: u32,
}

impl Form {
    pub fn of(s: &Settings) -> Self {
        Self {
            push_to_talk: s.push_to_talk,
            autostart: s.autostart,
            language: s.language,
            script: s.script,
            engines: s.engines.clone(),
            dictionary: s.dictionary.clone(),
            polish: s.polish.enabled,
            tone: s.polish.tone.clone(),
            polish_apps: s.polish.apps.clone(),
            history: s.history.enabled,
            retention_days: s.history.retention_days,
        }
    }

    /// `settings` with this form's values, tidied:
    /// - engines trimmed, blanks and repeats dropped; none at all keeps the old chain;
    /// - dictionary terms trimmed, blanks and case-insensitive repeats dropped;
    /// - an unknown tone becomes `neutral`;
    /// - per-app keys lower-cased with `.exe`;
    /// - retention at least one day.
    pub fn apply(self, settings: &Settings) -> Settings {
        let mut s = settings.clone();
        s.push_to_talk = self.push_to_talk;
        s.autostart = self.autostart;
        s.language = self.language;
        s.script = self.script;
        let engines = dedupe(self.engines, |e| e.to_owned());
        if !engines.is_empty() {
            s.engines = engines;
        }
        s.dictionary = dedupe(self.dictionary, str::to_lowercase);
        s.polish.enabled = self.polish;
        let tone = self.tone.trim().to_lowercase();
        s.polish.tone = if TONES.contains(&tone.as_str()) { tone } else { "neutral".into() };
        s.polish.apps = self
            .polish_apps
            .into_iter()
            .map(|(exe, on)| (exe_name(&exe), on))
            .filter(|(exe, _)| exe != ".exe")
            .collect();
        s.history.enabled = self.history;
        s.history.retention_days = self.retention_days.max(1);
        s
    }
}

/// `exe` as the app rules match it: lower case, ending in `.exe`.
pub fn exe_name(exe: &str) -> String {
    let exe = exe.trim().to_lowercase();
    if exe.ends_with(".exe") { exe } else { format!("{exe}.exe") }
}

/// Trimmed, without blanks, without repeats by `key` (the first one wins).
fn dedupe(items: Vec<String>, key: impl Fn(&str) -> String) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    items.into_iter().map(|i| i.trim().to_owned()).filter(|i| !i.is_empty() && seen.insert(key(i))).collect()
}

/// What a settings change asks of the running app.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Changes {
    /// The engine needs the new config (engines, language, script, dictionary or the
    /// models directory). Its model reloads on the next dictation.
    pub engine: bool,
    /// The polish choices the app plans each dictation with.
    pub polish: bool,
    /// The push-to-talk hook needs reinstalling with the new key.
    pub key: bool,
    pub autostart: bool,
    /// History recording on or off.
    pub history: bool,
    /// Prune now with the new retention.
    pub retention: bool,
}

impl Changes {
    pub fn any(&self) -> bool {
        *self != Self::default()
    }
}

/// What changed from `old` to `new`.
pub fn changes(old: &Settings, new: &Settings) -> Changes {
    Changes {
        engine: old.engines != new.engines
            || old.language != new.language
            || old.script != new.script
            || old.dictionary != new.dictionary
            || old.models_dir != new.models_dir,
        polish: old.polish != new.polish || old.dictionary != new.dictionary,
        key: old.push_to_talk != new.push_to_talk,
        autostart: old.autostart != new.autostart,
        history: old.history.enabled != new.history.enabled,
        retention: old.history.retention_days != new.history.retention_days,
    }
}

/// The engine specs the window offers besides local models, with what they are.
pub const CLOUD_ENGINES: [(&str, &str, &str); 4] = [
    ("deepgram-stream", "Deepgram, streaming", "deepgram"),
    ("deepgram", "Deepgram", "deepgram"),
    ("groq", "Groq Whisper", "groq"),
    ("sarvam", "Sarvam", "sarvam"),
];

/// The provider whose key engine `spec` needs, if any.
pub fn provider_of(spec: &str) -> Option<&'static str> {
    let kind = spec.split_once(':').map_or(spec, |(k, _)| k);
    CLOUD_ENGINES.iter().find(|(s, ..)| *s == kind).map(|(.., p)| *p)
}

#[cfg(test)]
mod tests {
    use bolo_core::{HinglishScript, LanguageMode};
    use bolo_store::{PushToTalkKey, Settings};

    use super::{Changes, Form, changes, exe_name, provider_of};

    #[test]
    fn a_form_round_trips_and_keeps_what_it_does_not_cover() {
        let mut s = Settings::default();
        s.overlay.position = 0.2;
        s.unknown.insert("future".into(), toml::Value::Boolean(true));
        let back = Form::of(&s).apply(&s);
        assert_eq!(back, s);
    }

    #[test]
    fn apply_tidies_the_values() {
        let s = Settings::default();
        let mut f = Form::of(&s);
        f.engines = vec![" local:moonshine-tiny ".into(), "".into(), "deepgram".into(), "deepgram".into()];
        f.dictionary = vec!["Bolo".into(), " bolo ".into(), "  ".into(), "Nirav".into()];
        f.tone = "Shouty".into();
        f.polish_apps = [("Slack".into(), false), ("code.exe".into(), true), (" ".into(), true)].into();
        f.retention_days = 0;
        let s = f.apply(&s);
        assert_eq!(s.engines, ["local:moonshine-tiny", "deepgram"]);
        assert_eq!(s.dictionary, ["Bolo", "Nirav"]);
        assert_eq!(s.polish.tone, "neutral");
        assert_eq!(s.polish.apps, [("code.exe".into(), true), ("slack.exe".into(), false)].into());
        assert_eq!(s.history.retention_days, 1);

        let mut f = Form::of(&s);
        f.engines = vec![" ".into()];
        f.tone = "Formal".into();
        let again = f.apply(&s);
        assert_eq!(again.engines, ["local:moonshine-tiny", "deepgram"], "an empty chain keeps the old one");
        assert_eq!(again.polish.tone, "formal");
    }

    #[test]
    fn changes_say_what_the_app_must_redo() {
        let old = Settings::default();
        let with = |f: fn(&mut Form)| {
            let mut form = Form::of(&old);
            f(&mut form);
            changes(&old, &form.apply(&old))
        };
        assert!(!changes(&old, &old).any());
        assert_eq!(with(|f| f.language = LanguageMode::Hinglish), Changes { engine: true, ..Default::default() });
        assert_eq!(with(|f| f.script = HinglishScript::Devanagari), Changes { engine: true, ..Default::default() });
        assert_eq!(with(|f| f.engines = vec!["deepgram".into()]), Changes { engine: true, ..Default::default() });
        assert_eq!(
            with(|f| f.dictionary = vec!["Bolo".into()]),
            Changes { engine: true, polish: true, ..Default::default() }
        );
        assert_eq!(with(|f| f.polish = true), Changes { polish: true, ..Default::default() });
        assert_eq!(with(|f| f.tone = "casual".into()), Changes { polish: true, ..Default::default() });
        assert_eq!(
            with(|f| f.polish_apps = [("slack.exe".into(), false)].into()),
            Changes { polish: true, ..Default::default() }
        );
        assert_eq!(with(|f| f.push_to_talk = PushToTalkKey::F13), Changes { key: true, ..Default::default() });
        assert_eq!(with(|f| f.autostart = true), Changes { autostart: true, ..Default::default() });
        assert_eq!(with(|f| f.history = false), Changes { history: true, ..Default::default() });
        assert_eq!(with(|f| f.retention_days = 7), Changes { retention: true, ..Default::default() });
    }

    #[test]
    fn exe_names_and_providers() {
        assert_eq!(exe_name(" WhatsApp "), "whatsapp.exe");
        assert_eq!(exe_name("Code.EXE"), "code.exe");
        assert_eq!(provider_of("deepgram-stream"), Some("deepgram"));
        assert_eq!(provider_of("groq:whisper-large-v3"), Some("groq"));
        assert_eq!(provider_of("sarvam"), Some("sarvam"));
        assert_eq!(provider_of("local:nemotron-stream"), None);
    }

    #[test]
    fn the_form_serializes_as_the_window_reads_it() {
        let json = serde_json::to_value(Form::of(&Settings::default())).unwrap();
        assert_eq!(json["push_to_talk"], "caps-lock");
        assert_eq!(json["language"], "auto");
        assert_eq!(json["script"], "roman");
        assert_eq!(json["retention_days"], 30);
    }
}
