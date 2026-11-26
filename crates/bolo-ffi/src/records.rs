//! The plain records Kotlin sees for settings, app profiles and history, and how they
//! map onto `bolo-store`. Saving goes through the same tidying as the desktop's settings
//! window (`bolo_store::form`).

use std::collections::HashSet;

use bolo_core::{HinglishScript, LanguageMode, OutputFormat, Snippet};
use bolo_store::apps::{self, AppKey, AppKind, AppRules, AppView, Defaults};
use bolo_store::form::{self, AppChange, General};
use bolo_store::{AppProfile, HistoryRow, Settings};

/// Which language a dictation is in (GLOSSARY: Language mode).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Language {
    Auto,
    English,
    Hinglish,
}

impl From<Language> for LanguageMode {
    fn from(l: Language) -> Self {
        match l {
            Language::Auto => LanguageMode::Auto,
            Language::English => LanguageMode::English,
            Language::Hinglish => LanguageMode::Hinglish,
        }
    }
}

impl From<LanguageMode> for Language {
    fn from(l: LanguageMode) -> Self {
        match l {
            LanguageMode::Auto => Language::Auto,
            LanguageMode::English => Language::English,
            LanguageMode::Hinglish => Language::Hinglish,
        }
    }
}

/// How Hinglish is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Script {
    Roman,
    Devanagari,
}

impl From<Script> for HinglishScript {
    fn from(s: Script) -> Self {
        match s {
            Script::Roman => HinglishScript::Roman,
            Script::Devanagari => HinglishScript::Devanagari,
        }
    }
}

impl From<HinglishScript> for Script {
    fn from(s: HinglishScript) -> Self {
        match s {
            HinglishScript::Roman => Script::Roman,
            HinglishScript::Devanagari => Script::Devanagari,
        }
    }
}

/// How an app wants its text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Format {
    Plain,
    Markdown,
    Html,
}

impl From<Format> for OutputFormat {
    fn from(f: Format) -> Self {
        match f {
            Format::Plain => OutputFormat::Plain,
            Format::Markdown => OutputFormat::Markdown,
            Format::Html => OutputFormat::Html,
        }
    }
}

impl From<OutputFormat> for Format {
    fn from(f: OutputFormat) -> Self {
        match f {
            OutputFormat::Plain => Format::Plain,
            OutputFormat::Markdown => Format::Markdown,
            OutputFormat::Html => Format::Html,
        }
    }
}

/// Say `trigger`, and Bolo inserts `text` in its place.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SnippetEntry {
    pub trigger: String,
    pub text: String,
}

/// The user's choices for one Android app (an `[[apps]]` row with a `package`). An unset
/// field follows the bundled rule, then the default (ADR-0020).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AppProfileEntry {
    /// The package name, e.g. `com.whatsapp`.
    pub package: String,
    pub name: Option<String>,
    pub polish: Option<bool>,
    /// "neutral", "casual" or "formal".
    pub tone: Option<String>,
    pub format: Option<Format>,
}

/// The settings the Android app edits: the desktop's `settings.toml` v2, minus the
/// desktop-only parts (the push-to-talk key, autostart, the microphone, the overlay and
/// updates), which a save keeps as they are.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BoloSettings {
    pub language: Language,
    pub script: Script,
    /// Engine specs, primary first: `local:<model-id>`, `deepgram`, `groq[:model]`, ...
    pub engines: Vec<String>,
    /// Names and terms to spell right.
    pub dictionary: Vec<String>,
    pub snippets: Vec<SnippetEntry>,
    /// Polish on or off, everywhere an app profile doesn't say otherwise.
    pub polish: bool,
    pub tone: String,
    /// Record dictations in history.
    pub history: bool,
    pub retention_days: u32,
    /// The Android app profiles, in the order they were added.
    pub apps: Vec<AppProfileEntry>,
    /// First-run onboarding's step while it's unfinished; `None` once it's done.
    pub onboarding_step: Option<u32>,
}

impl BoloSettings {
    pub fn of(s: &Settings) -> Self {
        let g = General::of(s);
        Self {
            language: g.language.into(),
            script: g.script.into(),
            engines: g.engines,
            dictionary: g.dictionary,
            snippets: g.snippets.into_iter().map(|s| SnippetEntry { trigger: s.trigger, text: s.text }).collect(),
            polish: g.polish,
            tone: g.tone,
            history: g.history,
            retention_days: g.retention_days,
            apps: s
                .apps
                .iter()
                .filter_map(|p| {
                    let AppKey::Package(package) = p.key(AppKind::Package)? else { return None };
                    Some(AppProfileEntry {
                        package,
                        name: p.name.clone(),
                        polish: p.polish,
                        tone: p.tone.clone(),
                        format: p.format.map(Format::from),
                    })
                })
                .collect(),
            onboarding_step: s.onboarding_step,
        }
    }

    /// `settings` with these values, tidied the way the desktop's settings window tidies
    /// them. App rows: a package the record doesn't list loses its row (or just its
    /// `package`, if the row also names a desktop exe); rows without a package are kept
    /// as they are. An unknown tone or a bad package name is an error.
    pub fn apply(self, settings: &Settings) -> Result<Settings, String> {
        let general = General {
            language: self.language.into(),
            script: self.script.into(),
            engines: self.engines,
            dictionary: self.dictionary,
            snippets: self.snippets.into_iter().map(|s| Snippet { trigger: s.trigger, text: s.text }).collect(),
            polish: self.polish,
            tone: self.tone,
            history: self.history,
            retention_days: self.retention_days,
        };
        let mut s = general.apply(settings);
        s.onboarding_step = self.onboarding_step;

        let mut wanted: Vec<(AppKey, AppProfileEntry)> = Vec::new();
        let mut seen = HashSet::new();
        for entry in self.apps {
            let package = form::new_package(&entry.package)?;
            if seen.insert(package.clone()) {
                wanted.push((AppKey::Package(package), entry));
            }
        }
        let mut rows: Vec<AppProfile> = Vec::with_capacity(s.apps.len());
        for row in std::mem::take(&mut s.apps) {
            match row.key(AppKind::Package) {
                Some(key) if !wanted.iter().any(|(k, _)| *k == key) => {
                    if row.key(AppKind::Exe).is_some() {
                        rows.push(AppProfile { package: None, ..row });
                    }
                }
                _ => rows.push(row),
            }
        }
        for (key, entry) in wanted {
            let changes = [
                AppChange::Name(entry.name),
                AppChange::Polish(entry.polish),
                AppChange::Tone(entry.tone),
                AppChange::Format(entry.format.map(OutputFormat::from)),
            ];
            for change in changes {
                rows = form::set_app(&rows, &key, change.tidy()?);
            }
        }
        s.apps = rows;
        Ok(s)
    }
}

/// One app on the settings app's Apps page: an app the bundled rules know or the user
/// added, with its fields resolved.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AppEntry {
    /// The package the user's row is stored under.
    pub package: String,
    /// Every package the entry covers (WhatsApp and WhatsApp Business, say).
    pub packages: Vec<String>,
    pub name: String,
    pub polish: bool,
    pub tone: String,
    pub format: Format,
    /// The bundled rules know this app; resetting it keeps it listed.
    pub bundled: bool,
    /// The user set at least one field here.
    pub custom: bool,
}

impl From<AppView> for AppEntry {
    fn from(v: AppView) -> Self {
        Self {
            package: v.exe,
            packages: v.exes,
            name: v.fields.name.value,
            polish: v.fields.polish.value,
            tone: v.fields.tone.value,
            format: v.fields.format.value.into(),
            bundled: v.bundled,
            custom: v.custom,
        }
    }
}

/// The Apps page's entries for `settings`.
pub fn app_entries(rules: &AppRules, settings: &Settings) -> Vec<AppEntry> {
    let defaults = Defaults { polish: settings.polish.enabled, tone: &settings.polish.tone };
    apps::app_views(AppKind::Package, rules, &settings.apps, defaults).into_iter().map(AppEntry::from).collect()
}

/// One dictation in history.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct HistoryEntry {
    pub id: i64,
    /// Unix time in milliseconds.
    pub at_ms: i64,
    /// The app's package (or a desktop exe, in a copied database).
    pub app: String,
    /// `english`, `hinglish` or `auto`.
    pub language: String,
    pub engine: String,
    /// The instant text.
    pub text: String,
    pub polished: Option<String>,
}

impl From<HistoryRow> for HistoryEntry {
    fn from(r: HistoryRow) -> Self {
        let d = r.dictation;
        Self {
            id: r.id,
            at_ms: d.at_ms,
            app: d.app,
            language: d.language,
            engine: d.engine,
            text: d.final_text,
            polished: d.polished_text,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(package: &str) -> AppProfileEntry {
        AppProfileEntry { package: package.into(), name: None, polish: None, tone: None, format: None }
    }

    #[test]
    fn settings_round_trip_through_the_record() {
        let mut s = Settings::default();
        s.language = LanguageMode::Hinglish;
        s.script = HinglishScript::Devanagari;
        s.engines = vec!["local:moonshine-tiny".into(), "deepgram".into()];
        s.dictionary = vec!["Bolo".into()];
        s.snippets.push(bolo_store::SnippetRow::new("sig", "Thanks,\nPriya"));
        s.polish.enabled = true;
        s.polish.tone = "casual".into();
        s.history.retention_days = 7;
        s.onboarding_step = Some(3);
        s.apps.push(AppProfile { polish: Some(false), ..AppProfile::new("slack.exe") });
        s.apps.push(AppProfile {
            tone: Some("formal".into()),
            format: Some(OutputFormat::Markdown),
            ..AppProfile::for_app(&AppKey::package("com.whatsapp"))
        });
        // Desktop-only settings and unknown keys survive an Android save.
        s.push_to_talk = bolo_store::PushToTalkKey::F13;
        s.overlay.position = 0.2;
        s.unknown.insert("future".into(), toml::Value::Boolean(true));

        let record = BoloSettings::of(&s);
        assert_eq!(record.language, Language::Hinglish);
        assert_eq!(record.script, Script::Devanagari);
        assert_eq!(record.snippets, [SnippetEntry { trigger: "sig".into(), text: "Thanks,\nPriya".into() }]);
        assert_eq!(
            record.apps,
            [AppProfileEntry {
                package: "com.whatsapp".into(),
                name: None,
                polish: None,
                tone: Some("formal".into()),
                format: Some(Format::Markdown)
            }],
            "only the Android rows"
        );
        assert_eq!(record.onboarding_step, Some(3));
        assert_eq!(record.clone().apply(&s).unwrap(), s);

        // And through the file.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        record.apply(&s).unwrap().save(&path).unwrap();
        assert_eq!(BoloSettings::of(&Settings::load(&path).unwrap().settings), BoloSettings::of(&s));
    }

    #[test]
    fn a_save_is_tidied_like_the_desktop_window() {
        let s = Settings::default();
        let mut r = BoloSettings::of(&s);
        r.engines = vec![" local:moonshine-tiny ".into(), "".into(), "local:moonshine-tiny".into()];
        r.dictionary = vec!["Bolo".into(), " bolo ".into()];
        r.tone = "Shouty".into();
        r.retention_days = 0;
        r.snippets = vec![SnippetEntry { trigger: "  ".into(), text: "x".into() }];
        let saved = BoloSettings::of(&r.apply(&s).unwrap());
        assert_eq!(saved.engines, ["local:moonshine-tiny"]);
        assert_eq!(saved.dictionary, ["Bolo"]);
        assert_eq!(saved.tone, "neutral");
        assert_eq!(saved.retention_days, 1);
        assert!(saved.snippets.is_empty());
    }

    #[test]
    fn app_rows_are_replaced_tidied_and_checked() {
        let mut s = Settings::default();
        s.apps = vec![
            AppProfile { polish: Some(false), ..AppProfile::new("slack.exe") },
            AppProfile { tone: Some("casual".into()), ..AppProfile::for_app(&AppKey::package("com.old")) },
            AppProfile {
                package: Some("com.both".into()),
                polish: Some(true),
                unknown: [("emoji".to_owned(), toml::Value::Boolean(true))].into_iter().collect(),
                ..AppProfile::new("both.exe")
            },
            AppProfile {
                unknown: [("emoji".to_owned(), toml::Value::Boolean(true))].into_iter().collect(),
                ..AppProfile::for_app(&AppKey::package("com.slack"))
            },
        ];
        let mut r = BoloSettings::of(&s);
        r.apps = vec![
            AppProfileEntry {
                name: Some("  ".into()),
                tone: Some(" Formal ".into()),
                format: Some(Format::Markdown),
                ..entry(" COM.Slack ")
            },
            AppProfileEntry { polish: Some(false), ..entry("com.whatsapp") },
            AppProfileEntry { polish: Some(true), ..entry("com.whatsapp") },
        ];
        let saved = r.apply(&s).unwrap();
        assert_eq!(saved.apps.len(), 4);
        assert_eq!(saved.apps[0], s.apps[0], "the desktop row is untouched");
        assert_eq!(saved.apps[1].package, None, "com.both is gone, both.exe stays");
        assert_eq!((saved.apps[1].exe.as_str(), saved.apps[1].polish), ("both.exe", Some(true)));
        let slack = &saved.apps[2];
        assert_eq!(slack.package.as_deref(), Some("com.slack"));
        assert_eq!((slack.name.as_deref(), slack.tone.as_deref()), (None, Some("formal")));
        assert_eq!(slack.unknown.get("emoji").and_then(toml::Value::as_bool), Some(true), "unknown keys kept");
        assert_eq!(saved.apps[3].package.as_deref(), Some("com.whatsapp"));
        assert_eq!(saved.apps[3].polish, Some(false), "the first entry for a package wins");

        let mut bad = BoloSettings::of(&s);
        bad.apps = vec![AppProfileEntry { tone: Some("shouty".into()), ..entry("com.slack") }];
        assert!(bad.apply(&s).unwrap_err().contains("shouty"));
        let mut bad = BoloSettings::of(&s);
        bad.apps = vec![entry("not a package")];
        assert!(bad.apply(&s).is_err());
    }

    #[test]
    fn the_apps_page_lists_android_apps_with_their_profiles() {
        let mut s = Settings::default();
        s.polish.enabled = true;
        s.apps.push(AppProfile { polish: Some(false), ..AppProfile::for_app(&AppKey::package("com.slack")) });
        s.apps.push(AppProfile::new("figma.exe"));
        let entries = app_entries(&AppRules::bundled(), &s);
        assert_eq!(entries[0].packages, ["com.whatsapp", "com.whatsapp.w4b"]);
        assert_eq!((entries[0].name.as_str(), entries[0].polish, entries[0].format), ("WhatsApp", true, Format::Plain));
        let slack = entries.iter().find(|e| e.package == "com.slack").unwrap();
        assert_eq!((slack.polish, slack.format, slack.custom, slack.bundled), (false, Format::Markdown, true, true));
        assert!(entries.iter().all(|e| !e.package.ends_with(".exe")), "no desktop programs");
    }
}
