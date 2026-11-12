//! The settings window's form: the part of [`Settings`] the window edits, and what a
//! change to it means for the running app.
//!
//! The window sends the whole [`Form`] on every change (Windows Settings style: no Save
//! button). [`Form::apply`] tidies it into the stored settings, keeping everything the
//! form doesn't cover (the overlay, the models directory, unknown keys), and [`changes`]
//! says which parts of the app have to follow. Both are pure, so they're tested here.
//!
//! The Apps tab edits the `[[apps]]` rows one field at a time instead ([`set_app`],
//! [`reset_app`], [`add_app`]).

use bolo_core::{HinglishScript, LanguageMode, OutputFormat, Snippet};
use bolo_store::{AppProfile, PushToTalkKey, Settings, SnippetRow};
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
    /// The `[[snippets]]` rows, in order.
    pub snippets: Vec<Snippet>,
    pub polish: bool,
    pub tone: String,
    pub history: bool,
    pub retention_days: u32,
    /// The microphone by name; `None` for the Windows default.
    #[serde(default)]
    pub input_device: Option<String>,
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
            snippets: s.snippets.iter().map(|r| Snippet { trigger: r.trigger.clone(), text: r.text.clone() }).collect(),
            polish: s.polish.enabled,
            tone: s.polish.tone.clone(),
            history: s.history.enabled,
            retention_days: s.history.retention_days,
            input_device: s.audio.input_device.clone(),
        }
    }

    /// `settings` with this form's values, tidied:
    /// - engines trimmed, blanks and repeats dropped; none at all keeps the old chain;
    /// - dictionary terms trimmed, blanks and case-insensitive repeats dropped;
    /// - snippet triggers trimmed; rows with a blank trigger or text, or a trigger
    ///   already used (ignoring case), dropped; the text kept as written;
    /// - an unknown tone becomes `neutral`;
    /// - retention at least one day;
    /// - a blank microphone is the Windows default.
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
        s.snippets = snippet_rows(self.snippets, &settings.snippets);
        s.polish.enabled = self.polish;
        let tone = self.tone.trim().to_lowercase();
        s.polish.tone = if TONES.contains(&tone.as_str()) { tone } else { "neutral".into() };
        s.history.enabled = self.history;
        s.history.retention_days = self.retention_days.max(1);
        s.audio.input_device = self.input_device.map(|d| d.trim().to_owned()).filter(|d| !d.is_empty());
        s
    }
}

/// One field of an `[[apps]]` row, as the Apps tab sets it; `None` clears it, so the
/// bundled rule or the default applies again.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "field", content = "value", rename_all = "lowercase")]
pub enum AppChange {
    Name(Option<String>),
    Polish(Option<bool>),
    Tone(Option<String>),
    Format(Option<OutputFormat>),
}

impl AppChange {
    /// The change tidied: a blank name or tone clears the field; an unknown tone is an
    /// error.
    pub fn tidy(self) -> Result<Self, String> {
        let text = |s: Option<String>| s.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty());
        Ok(match self {
            Self::Name(n) => Self::Name(text(n)),
            Self::Tone(t) => match text(t).map(|t| t.to_lowercase()) {
                Some(t) if !TONES.contains(&t.as_str()) => return Err(format!("unknown tone {t}")),
                t => Self::Tone(t),
            },
            other => other,
        })
    }
}

/// `apps` with `change` made to `exe`'s row, which is added if there is none.
pub fn set_app(apps: &[AppProfile], exe: &str, change: AppChange) -> Vec<AppProfile> {
    let exe = bolo_store::exe_name(exe);
    let mut apps = apps.to_vec();
    let i = match apps.iter().position(|p| bolo_store::exe_name(&p.exe) == exe) {
        Some(i) => i,
        None => {
            apps.push(AppProfile::new(&exe));
            apps.len() - 1
        }
    };
    let row = &mut apps[i];
    match change {
        AppChange::Name(v) => row.name = v,
        AppChange::Polish(v) => row.polish = v,
        AppChange::Tone(v) => row.tone = v,
        AppChange::Format(v) => row.format = v,
    }
    apps
}

/// `apps` without `exe`'s row: a bundled app goes back to its rule, an added one goes.
pub fn reset_app(apps: &[AppProfile], exe: &str) -> Vec<AppProfile> {
    let exe = bolo_store::exe_name(exe);
    apps.iter().filter(|p| bolo_store::exe_name(&p.exe) != exe).cloned().collect()
}

/// A program name the user typed, as a row's `exe`: `whatsapp` → `whatsapp.exe`. Not a
/// path, and not blank.
pub fn new_exe(typed: &str) -> Result<String, String> {
    let exe = bolo_store::exe_name(typed);
    if exe == ".exe" {
        return Err("type the program's name, like whatsapp.exe".into());
    }
    if exe.contains(['\\', '/', ':', '*', '?', '"', '<', '>', '|']) {
        return Err(format!("{} is not a program name; type just the name, like whatsapp.exe", typed.trim()));
    }
    Ok(exe)
}

/// `apps` with an empty row for `exe` at the end, unless `listed` (the Apps tab has a
/// row for it already).
pub fn add_app(apps: &[AppProfile], exe: &str, listed: bool) -> Vec<AppProfile> {
    let mut apps = apps.to_vec();
    if !listed && !apps.iter().any(|p| bolo_store::exe_name(&p.exe) == exe) {
        apps.push(AppProfile::new(exe));
    }
    apps
}

/// The form's snippets as `[[snippets]]` rows, tidied (see [`Form::apply`]). A row
/// whose trigger was already stored keeps the keys this version doesn't know.
fn snippet_rows(snippets: Vec<Snippet>, old: &[SnippetRow]) -> Vec<SnippetRow> {
    let mut seen = std::collections::HashSet::new();
    snippets
        .into_iter()
        .filter_map(|s| {
            let trigger = s.trigger.trim();
            if trigger.is_empty() || s.text.trim().is_empty() || !seen.insert(trigger.to_lowercase()) {
                return None;
            }
            let unknown = old
                .iter()
                .find(|r| r.trigger.trim().eq_ignore_ascii_case(trigger))
                .map(|r| r.unknown.clone())
                .unwrap_or_default();
            Some(SnippetRow { trigger: trigger.to_owned(), text: s.text, unknown })
        })
        .collect()
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
    /// The snippets the engine expands. The model stays loaded.
    pub snippets: bool,
    /// The microphone. The model stays loaded; a warm mic reopens on the new device.
    pub mic: bool,
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
        polish: old.polish != new.polish || old.apps != new.apps || old.dictionary != new.dictionary,
        snippets: old.snippets != new.snippets,
        mic: old.audio.device() != new.audio.device(),
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

    use bolo_core::{OutputFormat, Snippet};
    use bolo_store::{AppProfile, SnippetRow};

    use super::{AppChange, Changes, Form, add_app, changes, new_exe, provider_of, reset_app, set_app};

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
        f.retention_days = 0;
        let s = f.apply(&s);
        assert_eq!(s.engines, ["local:moonshine-tiny", "deepgram"]);
        assert_eq!(s.dictionary, ["Bolo", "Nirav"]);
        assert_eq!(s.polish.tone, "neutral");
        assert_eq!(s.history.retention_days, 1);

        let mut f = Form::of(&s);
        f.engines = vec![" ".into()];
        f.tone = "Formal".into();
        let again = f.apply(&s);
        assert_eq!(again.engines, ["local:moonshine-tiny", "deepgram"], "an empty chain keeps the old one");
        assert_eq!(again.polish.tone, "formal");

        let mut f = Form::of(&again);
        f.input_device = Some(" USB Headset ".into());
        let usb = f.apply(&again);
        assert_eq!(usb.audio.input_device.as_deref(), Some("USB Headset"));
        let mut f = Form::of(&usb);
        f.input_device = Some("  ".into());
        assert_eq!(f.apply(&usb).audio.input_device, None, "blank is the Windows default");
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
        assert_eq!(
            with(|f| f.snippets = vec![Snippet { trigger: "sig".into(), text: "Thanks".into() }]),
            Changes { snippets: true, ..Default::default() },
            "snippets don't reload the model"
        );
        assert_eq!(with(|f| f.tone = "casual".into()), Changes { polish: true, ..Default::default() });
        assert_eq!(
            with(|f| f.input_device = Some("USB Headset".into())),
            Changes { mic: true, ..Default::default() },
            "a new mic doesn't reload the model"
        );
        let apps = Settings { apps: vec![AppProfile::new("slack.exe")], ..old.clone() };
        assert_eq!(changes(&old, &apps), Changes { polish: true, ..Default::default() }, "app rows replan polish");
        assert_eq!(with(|f| f.push_to_talk = PushToTalkKey::F13), Changes { key: true, ..Default::default() });
        assert_eq!(with(|f| f.autostart = true), Changes { autostart: true, ..Default::default() });
        assert_eq!(with(|f| f.history = false), Changes { history: true, ..Default::default() });
        assert_eq!(with(|f| f.retention_days = 7), Changes { retention: true, ..Default::default() });
    }

    #[test]
    fn snippets_are_tidied_and_keep_unknown_keys() {
        let mut row = SnippetRow::new("sig", "old");
        row.unknown.insert("future".into(), toml::Value::Boolean(true));
        let s = Settings { snippets: vec![row], ..Default::default() };
        let snippet = |t: &str, x: &str| Snippet { trigger: t.into(), text: x.into() };
        let mut f = Form::of(&s);
        assert_eq!(f.snippets, [snippet("sig", "old")]);
        f.snippets = vec![
            snippet(" SIG ", "Thanks,\nPriya \n"),
            snippet("sig", "again"),
            snippet("  ", "no trigger"),
            snippet("blank", "  "),
            snippet("my address", "221B"),
        ];
        let saved = f.apply(&s).snippets;
        assert_eq!(saved.len(), 2);
        assert_eq!((saved[0].trigger.as_str(), saved[0].text.as_str()), ("SIG", "Thanks,\nPriya \n"), "text verbatim");
        assert_eq!(saved[0].unknown.get("future").and_then(toml::Value::as_bool), Some(true));
        assert_eq!((saved[1].trigger.as_str(), saved[1].text.as_str()), ("my address", "221B"));
        assert!(saved[1].unknown.is_empty());
    }

    #[test]
    fn a_form_save_keeps_the_app_rows() {
        let s = Settings {
            apps: vec![AppProfile { polish: Some(false), ..AppProfile::new("slack.exe") }],
            ..Default::default()
        };
        let mut f = Form::of(&s);
        f.polish = true;
        assert_eq!(f.apply(&s).apps, s.apps);
    }

    #[test]
    fn app_rows_are_set_one_field_at_a_time_and_reset_whole() {
        let apps = set_app(&[], "Slack", AppChange::Format(Some(OutputFormat::Plain)));
        assert_eq!(apps, [AppProfile { format: Some(OutputFormat::Plain), ..AppProfile::new("slack.exe") }]);
        let apps = set_app(&apps, "slack.exe", AppChange::Tone(Some("casual".into())));
        let apps = set_app(&apps, "code.exe", AppChange::Polish(Some(true)));
        assert_eq!(apps.len(), 2);
        assert_eq!((apps[0].format, apps[0].tone.as_deref()), (Some(OutputFormat::Plain), Some("casual")));
        let apps = set_app(&apps, "SLACK.EXE", AppChange::Format(None));
        assert_eq!((apps[0].format, apps[0].tone.as_deref()), (None, Some("casual")), "None clears one field");
        let apps = reset_app(&apps, "slack");
        assert_eq!(apps, [AppProfile { polish: Some(true), ..AppProfile::new("code.exe") }]);
    }

    #[test]
    fn app_changes_are_tidied() {
        assert_eq!(AppChange::Name(Some("  ".into())).tidy(), Ok(AppChange::Name(None)));
        assert_eq!(AppChange::Name(Some(" Work ".into())).tidy(), Ok(AppChange::Name(Some("Work".into()))));
        assert_eq!(AppChange::Tone(Some("Formal".into())).tidy(), Ok(AppChange::Tone(Some("formal".into()))));
        assert!(AppChange::Tone(Some("shouty".into())).tidy().is_err());
        let json = serde_json::json!({ "field": "format", "value": "markdown" });
        assert_eq!(serde_json::from_value::<AppChange>(json).unwrap(), AppChange::Format(Some(OutputFormat::Markdown)));
        let json = serde_json::json!({ "field": "polish", "value": null });
        assert_eq!(serde_json::from_value::<AppChange>(json).unwrap(), AppChange::Polish(None));
    }

    #[test]
    fn apps_are_added_by_program_name_once() {
        assert_eq!(new_exe(" WhatsApp "), Ok("whatsapp.exe".into()));
        assert!(new_exe("  ").is_err());
        assert!(new_exe(r"C:\Program Files\x.exe").is_err());
        let apps = add_app(&[], "whatsapp.exe", false);
        assert_eq!(apps, [AppProfile::new("whatsapp.exe")]);
        assert_eq!(add_app(&apps, "whatsapp.exe", false), apps, "already a row");
        assert!(add_app(&[], "slack.exe", true).is_empty(), "the bundled rules list it");
    }

    #[test]
    fn providers() {
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
        assert_eq!(json["input_device"], serde_json::Value::Null);
    }
}
