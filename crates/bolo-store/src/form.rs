//! The settings form both apps edit: the part of [`Settings`] that isn't tied to one
//! platform, how a form is tidied into the stored settings, and what a change means for
//! a running app.
//!
//! The desktop's settings window adds its own fields (the push-to-talk key, autostart,
//! the microphone, updates) around [`General`]; the Android settings app sends it through
//! `bolo-ffi`. [`General::apply`] keeps everything the form doesn't cover (the overlay,
//! the models directory, unknown keys), and [`changes`] says which parts of the app have
//! to follow. Both are pure, so they're tested here.
//!
//! The Apps tab edits the `[[apps]]` rows one field at a time instead ([`set_app`],
//! [`reset_app`], [`add_app`]).

use bolo_core::{HinglishScript, LanguageMode, OutputFormat, Snippet};
use serde::{Deserialize, Serialize};

use crate::apps::AppKey;
use crate::settings::{AppProfile, Settings, SnippetRow, exe_name, package_name};

/// The tones polish knows.
pub const TONES: [&str; 3] = ["neutral", "casual", "formal"];

/// The history retention the settings offer, in days.
pub const RETENTION_DAYS: [u32; 5] = [1, 7, 30, 90, 365];

/// The settings both apps edit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct General {
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
}

impl General {
    pub fn of(s: &Settings) -> Self {
        Self {
            language: s.language,
            script: s.script,
            engines: s.engines.clone(),
            dictionary: s.dictionary.clone(),
            snippets: s.snippets.iter().map(|r| Snippet { trigger: r.trigger.clone(), text: r.text.clone() }).collect(),
            polish: s.polish.enabled,
            tone: s.polish.tone.clone(),
            history: s.history.enabled,
            retention_days: s.history.retention_days,
        }
    }

    /// `settings` with this form's values, tidied:
    /// - engines trimmed, blanks and repeats dropped; none at all keeps the old chain;
    /// - dictionary terms trimmed, blanks and case-insensitive repeats dropped;
    /// - snippet triggers trimmed; rows with a blank trigger or text, or a trigger
    ///   already used (ignoring case), dropped; the text kept as written;
    /// - an unknown tone becomes `neutral`;
    /// - retention at least one day.
    pub fn apply(self, settings: &Settings) -> Settings {
        let mut s = settings.clone();
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

/// `apps` with `change` made to `app`'s row, which is added if there is none.
pub fn set_app(apps: &[AppProfile], app: &AppKey, change: AppChange) -> Vec<AppProfile> {
    let mut apps = apps.to_vec();
    let i = match apps.iter().position(|p| p.matches(app)) {
        Some(i) => i,
        None => {
            apps.push(AppProfile::for_app(app));
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

/// `apps` without `app`'s row: a bundled app goes back to its rule, an added one goes.
pub fn reset_app(apps: &[AppProfile], app: &AppKey) -> Vec<AppProfile> {
    apps.iter().filter(|p| !p.matches(app)).cloned().collect()
}

/// `apps` with an empty row for `app` at the end, unless `listed` (the Apps tab has a
/// row for it already).
pub fn add_app(apps: &[AppProfile], app: &AppKey, listed: bool) -> Vec<AppProfile> {
    let mut apps = apps.to_vec();
    if !listed && !apps.iter().any(|p| p.matches(app)) {
        apps.push(AppProfile::for_app(app));
    }
    apps
}

/// A program name the user typed, as a row's `exe`: `whatsapp` → `whatsapp.exe`. Not a
/// path, and not blank.
pub fn new_exe(typed: &str) -> Result<String, String> {
    let exe = exe_name(typed);
    if exe == ".exe" {
        return Err("type the program's name, like whatsapp.exe".into());
    }
    if exe.contains(['\\', '/', ':', '*', '?', '"', '<', '>', '|']) {
        return Err(format!("{} is not a program name; type just the name, like whatsapp.exe", typed.trim()));
    }
    Ok(exe)
}

/// An Android package name as given (the settings app picks installed apps, so this
/// guards against a bad record): two or more dot-separated parts, each a letter then
/// letters, digits or underscores. Lower-cased by [`package_name`].
pub fn new_package(typed: &str) -> Result<String, String> {
    let package = package_name(typed);
    let part_ok = |p: &str| {
        let mut chars = p.chars();
        chars.next().is_some_and(|c| c.is_ascii_alphabetic()) && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
    };
    if package.split('.').count() < 2 || !package.split('.').all(part_ok) {
        return Err(format!("{:?} is not an Android package name, like com.whatsapp", typed.trim()));
    }
    Ok(package)
}

/// The form's snippets as `[[snippets]]` rows, tidied (see [`General::apply`]). A row
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

/// What a change to the shared settings asks of a running app.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Changes {
    /// The engine needs the new config (engines, language, script, dictionary or the
    /// models directory). Its model is dropped and loaded again (see [`Changes::warm`]).
    pub engine: bool,
    /// The polish choices each dictation is planned with.
    pub polish: bool,
    /// The snippets the engine expands. The model stays loaded.
    pub snippets: bool,
    /// History recording on or off.
    pub history: bool,
    /// Prune now with the new retention.
    pub retention: bool,
    /// Load the model now, ready for the next dictation: the engine config changed, or
    /// onboarding just finished (the first time a model is surely there).
    pub warm: bool,
}

impl Changes {
    pub fn any(&self) -> bool {
        *self != Self::default()
    }
}

/// What changed from `old` to `new`, in the settings both apps share.
pub fn changes(old: &Settings, new: &Settings) -> Changes {
    let engine = old.engines != new.engines
        || old.language != new.language
        || old.script != new.script
        || old.dictionary != new.dictionary
        || old.models_dir != new.models_dir;
    Changes {
        engine,
        polish: old.polish != new.polish || old.apps != new.apps || old.dictionary != new.dictionary,
        snippets: old.snippets != new.snippets,
        history: old.history.enabled != new.history.enabled,
        retention: old.history.retention_days != new.history.retention_days,
        warm: engine || (old.onboarding_step.is_some() && new.onboarding_step.is_none()),
    }
}

/// The engine specs the settings offer besides local models, with what they are.
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
    use super::*;

    #[test]
    fn a_form_round_trips_and_keeps_what_it_does_not_cover() {
        let mut s = Settings::default();
        s.overlay.position = 0.2;
        s.push_to_talk = crate::PushToTalkKey::F13;
        s.unknown.insert("future".into(), toml::Value::Boolean(true));
        assert_eq!(General::of(&s).apply(&s), s);
    }

    #[test]
    fn apply_tidies_the_values() {
        let s = Settings::default();
        let mut f = General::of(&s);
        f.engines = vec![" local:moonshine-tiny ".into(), "".into(), "deepgram".into(), "deepgram".into()];
        f.dictionary = vec!["Bolo".into(), " bolo ".into(), "  ".into(), "Nirav".into()];
        f.tone = "Shouty".into();
        f.retention_days = 0;
        let s = f.apply(&s);
        assert_eq!(s.engines, ["local:moonshine-tiny", "deepgram"]);
        assert_eq!(s.dictionary, ["Bolo", "Nirav"]);
        assert_eq!(s.polish.tone, "neutral");
        assert_eq!(s.history.retention_days, 1);

        let mut f = General::of(&s);
        f.engines = vec![" ".into()];
        f.tone = "Formal".into();
        let again = f.apply(&s);
        assert_eq!(again.engines, ["local:moonshine-tiny", "deepgram"], "an empty chain keeps the old one");
        assert_eq!(again.polish.tone, "formal");
    }

    #[test]
    fn changes_say_what_the_app_must_redo() {
        let old = Settings::default();
        let with = |f: fn(&mut General)| {
            let mut form = General::of(&old);
            f(&mut form);
            changes(&old, &form.apply(&old))
        };
        assert!(!changes(&old, &old).any());
        let engine = Changes { engine: true, warm: true, ..Default::default() };
        assert_eq!(with(|f| f.language = LanguageMode::Hinglish), engine);
        assert_eq!(with(|f| f.script = HinglishScript::Devanagari), engine);
        assert_eq!(with(|f| f.engines = vec!["deepgram".into()]), engine);
        assert_eq!(with(|f| f.dictionary = vec!["Bolo".into()]), Changes { polish: true, ..engine });
        assert_eq!(with(|f| f.polish = true), Changes { polish: true, ..Default::default() });
        assert_eq!(with(|f| f.tone = "casual".into()), Changes { polish: true, ..Default::default() });
        assert_eq!(
            with(|f| f.snippets = vec![Snippet { trigger: "sig".into(), text: "Thanks".into() }]),
            Changes { snippets: true, ..Default::default() },
            "snippets don't reload the model"
        );
        assert_eq!(with(|f| f.history = false), Changes { history: true, ..Default::default() });
        assert_eq!(with(|f| f.retention_days = 7), Changes { retention: true, ..Default::default() });
        let apps = Settings { apps: vec![AppProfile::for_app(&AppKey::package("com.slack"))], ..old.clone() };
        assert_eq!(changes(&old, &apps), Changes { polish: true, ..Default::default() }, "app rows replan polish");
        let onboarding = Settings { onboarding_step: Some(2), ..old.clone() };
        assert_eq!(changes(&onboarding, &old), Changes { warm: true, ..Default::default() }, "onboarding done: load");
        assert!(!changes(&old, &onboarding).any(), "starting onboarding loads nothing");
    }

    #[test]
    fn snippets_are_tidied_and_keep_unknown_keys() {
        let mut row = SnippetRow::new("sig", "old");
        row.unknown.insert("future".into(), toml::Value::Boolean(true));
        let s = Settings { snippets: vec![row], ..Default::default() };
        let snippet = |t: &str, x: &str| Snippet { trigger: t.into(), text: x.into() };
        let mut f = General::of(&s);
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
    fn package_rows_are_set_one_field_at_a_time_and_reset_whole() {
        let whatsapp = AppKey::package("com.whatsapp");
        let slack_exe = AppKey::Exe(exe_name("slack"));
        let apps = set_app(&[AppProfile::new("slack.exe")], &whatsapp, AppChange::Tone(Some("casual".into())));
        let apps = set_app(&apps, &AppKey::package(" COM.WhatsApp "), AppChange::Format(Some(OutputFormat::Plain)));
        assert_eq!(apps.len(), 2, "the exe row is another app's");
        assert_eq!(apps[1].package.as_deref(), Some("com.whatsapp"));
        assert_eq!((apps[1].tone.as_deref(), apps[1].format), (Some("casual"), Some(OutputFormat::Plain)));
        assert_eq!(apps[1].exe, "");
        let apps = reset_app(&apps, &whatsapp);
        assert_eq!(apps, [AppProfile::new("slack.exe")]);
        assert_eq!(add_app(&apps, &whatsapp, false).len(), 2);
        assert_eq!(add_app(&apps, &slack_exe, false), apps, "already a row");
        assert_eq!(add_app(&apps, &whatsapp, true), apps, "the bundled rules list it");
    }

    #[test]
    fn package_names_are_checked() {
        assert_eq!(new_package(" com.Slack "), Ok("com.slack".into()));
        assert_eq!(new_package("com.whatsapp.w4b"), Ok("com.whatsapp.w4b".into()));
        for bad in ["", "whatsapp", "com..x", "com.1x", "com/x", "com.x-y"] {
            assert!(new_package(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn providers() {
        assert_eq!(provider_of("deepgram-stream"), Some("deepgram"));
        assert_eq!(provider_of("groq:whisper-large-v3"), Some("groq"));
        assert_eq!(provider_of("sarvam"), Some("sarvam"));
        assert_eq!(provider_of("local:nemotron-stream"), None);
    }
}
