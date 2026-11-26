//! The settings window's form: the part of [`Settings`] the window edits, and what a
//! change to it means for the running app.
//!
//! The window sends the whole [`Form`] on every change (Windows Settings style: no Save
//! button). The settings both apps share, and how they're tidied, are
//! `bolo_store::form`'s [`General`]; this adds the desktop's own fields around it (the
//! push-to-talk key, autostart, the microphone, updates). [`Form::apply`] keeps
//! everything the form doesn't cover (the overlay, the models directory, unknown keys),
//! and [`changes`] says which parts of the app have to follow. Both are pure, so they're
//! tested here.
//!
//! The Apps tab edits the `[[apps]]` rows one field at a time instead ([`set_app`],
//! [`reset_app`], [`add_app`]), by program name.

use bolo_store::form::{self as shared, General};
pub use bolo_store::form::{AppChange, CLOUD_ENGINES, new_exe, provider_of};
use bolo_store::{AppKey, AppProfile, Channel, PushToTalkKey, Settings, exe_name};
use serde::{Deserialize, Serialize};

use crate::variant::Variant;

/// What the settings window edits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Form {
    /// Language, engines, dictionary, snippets, polish and history.
    #[serde(flatten)]
    pub general: General,
    pub push_to_talk: PushToTalkKey,
    pub autostart: bool,
    /// The microphone by name; `None` for the Windows default.
    #[serde(default)]
    pub input_device: Option<String>,
    /// `[updates] auto_check`: check for updates at launch and once a day.
    pub auto_update_check: bool,
    /// `[updates] channel`: the channel Bolo follows (unpicked, the build's own).
    pub channel: Channel,
}

impl Form {
    pub fn of(s: &Settings) -> Self {
        Self {
            general: General::of(s),
            push_to_talk: s.push_to_talk,
            autostart: s.autostart,
            input_device: s.audio.input_device.clone(),
            auto_update_check: s.updates.auto_check,
            channel: Variant::followed(s.updates.channel).channel(),
        }
    }

    /// `settings` with this form's values, tidied as [`General::apply`] says, and:
    /// - a blank microphone is the Windows default;
    /// - the channel is stored once it differs from the one followed so far.
    pub fn apply(self, settings: &Settings) -> Settings {
        let mut s = self.general.apply(settings);
        s.push_to_talk = self.push_to_talk;
        s.autostart = self.autostart;
        s.audio.input_device = self.input_device.map(|d| d.trim().to_owned()).filter(|d| !d.is_empty());
        s.updates.auto_check = self.auto_update_check;
        if self.channel != Variant::followed(settings.updates.channel).channel() {
            s.updates.channel = Some(self.channel);
        }
        s
    }
}

/// The row key for a program name as typed: `Slack` → `slack.exe`.
fn exe_key(exe: &str) -> AppKey {
    AppKey::Exe(exe_name(exe))
}

/// `apps` with `change` made to `exe`'s row, which is added if there is none.
pub fn set_app(apps: &[AppProfile], exe: &str, change: AppChange) -> Vec<AppProfile> {
    shared::set_app(apps, &exe_key(exe), change)
}

/// `apps` without `exe`'s row: a bundled app goes back to its rule, an added one goes.
pub fn reset_app(apps: &[AppProfile], exe: &str) -> Vec<AppProfile> {
    shared::reset_app(apps, &exe_key(exe))
}

/// `apps` with an empty row for `exe` at the end, unless `listed` (the Apps tab has a
/// row for it already).
pub fn add_app(apps: &[AppProfile], exe: &str, listed: bool) -> Vec<AppProfile> {
    shared::add_app(apps, &exe_key(exe), listed)
}

/// What a settings change asks of the running app.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Changes {
    /// The engine needs the new config (engines, language, script, dictionary or the
    /// models directory). Its model is dropped and loaded again (see [`Changes::warm`]).
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
    /// The channel Bolo follows changed: switch to that channel's build.
    pub channel: bool,
    /// Load the model now, ready for the next push-to-talk: the engine config changed,
    /// or onboarding just finished (the first time a model is surely there).
    pub warm: bool,
}

impl Changes {
    pub fn any(&self) -> bool {
        *self != Self::default()
    }
}

/// What changed from `old` to `new`.
pub fn changes(old: &Settings, new: &Settings) -> Changes {
    let shared = shared::changes(old, new);
    Changes {
        engine: shared.engine,
        polish: shared.polish,
        snippets: shared.snippets,
        mic: old.audio.device() != new.audio.device(),
        key: old.push_to_talk != new.push_to_talk,
        autostart: old.autostart != new.autostart,
        history: shared.history,
        retention: shared.retention,
        channel: Variant::followed(old.updates.channel) != Variant::followed(new.updates.channel),
        warm: shared.warm,
    }
}

#[cfg(test)]
mod tests {
    use bolo_core::{HinglishScript, LanguageMode};
    use bolo_store::{Channel, PushToTalkKey, Settings};

    use crate::variant::Variant;

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
        f.general.engines = vec![" local:moonshine-tiny ".into(), "".into(), "deepgram".into(), "deepgram".into()];
        f.general.dictionary = vec!["Bolo".into(), " bolo ".into(), "  ".into(), "Nirav".into()];
        f.general.tone = "Shouty".into();
        f.general.retention_days = 0;
        let s = f.apply(&s);
        assert_eq!(s.engines, ["local:moonshine-tiny", "deepgram"]);
        assert_eq!(s.dictionary, ["Bolo", "Nirav"]);
        assert_eq!(s.polish.tone, "neutral");
        assert_eq!(s.history.retention_days, 1);

        let mut f = Form::of(&s);
        f.general.engines = vec![" ".into()];
        f.general.tone = "Formal".into();
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
        assert_eq!(
            with(|f| f.general.language = LanguageMode::Hinglish),
            Changes { engine: true, warm: true, ..Default::default() }
        );
        assert_eq!(
            with(|f| f.general.script = HinglishScript::Devanagari),
            Changes { engine: true, warm: true, ..Default::default() }
        );
        assert_eq!(
            with(|f| f.general.engines = vec!["deepgram".into()]),
            Changes { engine: true, warm: true, ..Default::default() }
        );
        assert_eq!(
            with(|f| f.general.dictionary = vec!["Bolo".into()]),
            Changes { engine: true, polish: true, warm: true, ..Default::default() }
        );
        assert_eq!(with(|f| f.general.polish = true), Changes { polish: true, ..Default::default() });
        assert_eq!(
            with(|f| f.general.snippets = vec![Snippet { trigger: "sig".into(), text: "Thanks".into() }]),
            Changes { snippets: true, ..Default::default() },
            "snippets don't reload the model"
        );
        assert_eq!(with(|f| f.general.tone = "casual".into()), Changes { polish: true, ..Default::default() });
        assert_eq!(
            with(|f| f.input_device = Some("USB Headset".into())),
            Changes { mic: true, ..Default::default() },
            "a new mic doesn't reload the model"
        );
        let apps = Settings { apps: vec![AppProfile::new("slack.exe")], ..old.clone() };
        assert_eq!(changes(&old, &apps), Changes { polish: true, ..Default::default() }, "app rows replan polish");
        assert_eq!(with(|f| f.push_to_talk = PushToTalkKey::F13), Changes { key: true, ..Default::default() });
        assert_eq!(with(|f| f.autostart = true), Changes { autostart: true, ..Default::default() });
        assert_eq!(with(|f| f.general.history = false), Changes { history: true, ..Default::default() });
        assert_eq!(with(|f| f.general.retention_days = 7), Changes { retention: true, ..Default::default() });
        assert_eq!(
            with(|f| {
                f.channel = match Variant::current() {
                    Variant::Release => Channel::Nightly,
                    Variant::Nightly => Channel::Beta,
                }
            }),
            Changes { channel: true, ..Default::default() },
            "picking the other channel switches"
        );
        assert!(!with(|f| f.channel = Variant::current().channel()).any(), "the channel already followed");
        let onboarding = Settings { onboarding_step: Some(2), ..old.clone() };
        assert_eq!(changes(&onboarding, &old), Changes { warm: true, ..Default::default() }, "onboarding done: load");
        assert!(!changes(&old, &onboarding).any(), "starting onboarding loads nothing");
        // The update schedule reads the switch at every tick: nothing to redo.
        assert!(!with(|f| f.auto_update_check = false).any());
        let mut f = Form::of(&old);
        f.auto_update_check = false;
        assert!(!f.apply(&old).updates.auto_check);
    }

    #[test]
    fn snippets_are_tidied_and_keep_unknown_keys() {
        let mut row = SnippetRow::new("sig", "old");
        row.unknown.insert("future".into(), toml::Value::Boolean(true));
        let s = Settings { snippets: vec![row], ..Default::default() };
        let snippet = |t: &str, x: &str| Snippet { trigger: t.into(), text: x.into() };
        let mut f = Form::of(&s);
        assert_eq!(f.general.snippets, [snippet("sig", "old")]);
        f.general.snippets = vec![
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
        f.general.polish = true;
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
        assert_eq!(json["auto_update_check"], true);
        assert_eq!(json["channel"], if Variant::current() == Variant::Nightly { "nightly" } else { "beta" });
    }
}
