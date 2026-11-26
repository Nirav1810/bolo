//! App rules on the desktop: the window in front decides how a dictation's text goes in.
//!
//! The table and the resolution live in `bolo_store::apps`, shared with the Android
//! keyboard. Here they're keyed by the foreground exe and window title, and the result
//! gains what only the desktop has: the inserter's [`Target`] (the paste chord and
//! multi-line swap safety).

use bolo_core::{Polish, TargetContext};
use bolo_store::AppProfile;
use bolo_store::apps::{self, AppKey, AppKind, Paste};
pub use bolo_store::apps::{AppRules, AppView, Defaults, PolishPrefs};

use crate::insert::{Chord, Target};

impl From<Paste> for Chord {
    fn from(p: Paste) -> Self {
        match p {
            Paste::CtrlV => Chord::CtrlV,
            Paste::ShiftInsert => Chord::ShiftInsert,
        }
    }
}

/// What the app does for a dictation into one window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    /// For the engine: the exe, the title (browsers only), the format and `code_mode`.
    pub ctx: TargetContext,
    /// For the inserter: the format, the paste chord and multi-line swap safety.
    pub target: Target,
    /// Whether polish should run here. The app still needs the Anthropic key.
    pub polish: bool,
    /// Polish's tone here.
    pub tone: String,
    /// The app's name for polish: the user's or the rule's name, else the exe without
    /// `.exe`.
    pub app_name: String,
}

fn target(r: &apps::Resolved) -> Target {
    Target {
        format: r.ctx.format,
        chord: r.paste.map(Chord::from).unwrap_or_default(),
        multiline_swap_safe: r.multiline_swap_safe,
    }
}

/// The target for the window in front: `exe` is its image name, `title` its title.
/// `profiles` are the user's `[[apps]]` rows; they win over the bundled rules field by
/// field (ADR-0020). Code mode, the paste chord and swap safety are the rules' alone.
pub fn resolve(exe: &str, title: &str, rules: &AppRules, profiles: &[AppProfile], defaults: Defaults) -> Resolved {
    let r = apps::resolve(&AppKey::exe(exe), title, rules, profiles, defaults);
    Resolved { target: target(&r), ctx: r.ctx, polish: r.polish, tone: r.tone, app_name: r.app_name }
}

/// The Apps tab's rows: the bundled rules' programs in table order, then the programs
/// only the user's rows name, in the order they were added.
pub fn app_views(rules: &AppRules, profiles: &[AppProfile], defaults: Defaults) -> Vec<AppView> {
    apps::app_views(AppKind::Exe, rules, profiles, defaults)
}

/// What a dictation into one window needs: the engine's target context and polish
/// config, and the inserter's target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub ctx: TargetContext,
    pub polish: Polish,
    pub target: Target,
}

/// The [`Plan`] for the window `exe` / `title`.
pub fn plan(exe: &str, title: &str, rules: &AppRules, prefs: &PolishPrefs) -> Plan {
    let p = apps::plan(&AppKey::exe(exe), title, rules, prefs);
    Plan { target: target(&p.resolved), ctx: p.resolved.ctx, polish: p.polish }
}

#[cfg(test)]
mod tests {
    use bolo_core::OutputFormat::{self, Html, Markdown, Plain};
    use bolo_core::Polish;
    use bolo_store::AppProfile;

    use bolo_store::apps::Source;

    use super::{AppRules, Defaults, PolishPrefs, Resolved, app_views, plan, resolve};
    use crate::insert::Chord;

    fn defaults(polish: bool) -> Defaults<'static> {
        Defaults { polish, tone: "neutral" }
    }

    fn bundled(exe: &str, title: &str, profiles: &[AppProfile], polish_default: bool) -> Resolved {
        resolve(exe, title, &AppRules::bundled(), profiles, defaults(polish_default))
    }

    /// A user row for `exe` with only polish set.
    fn polish(exe: &str, on: bool) -> AppProfile {
        AppProfile { polish: Some(on), ..AppProfile::new(exe) }
    }

    /// (exe, title) → (format, code_mode, polish with the global switch on, chord, name).
    #[test]
    fn the_bundled_table() {
        let cases: &[(&str, &str, OutputFormat, bool, bool, Chord, &str)] = &[
            ("code.exe", "main.rs - bolo - Visual Studio Code", Plain, true, false, Chord::CtrlV, "VS Code"),
            ("Cursor.exe", "x", Plain, true, false, Chord::CtrlV, "Cursor"),
            ("idea64.exe", "", Plain, true, false, Chord::CtrlV, "IntelliJ IDEA"),
            ("devenv.exe", "", Plain, true, false, Chord::CtrlV, "Visual Studio"),
            ("WindowsTerminal.exe", "pwsh", Plain, true, false, Chord::CtrlV, "Terminal"),
            ("wezterm-gui.exe", "", Plain, true, false, Chord::CtrlV, "Terminal"),
            ("alacritty.exe", "", Plain, true, false, Chord::CtrlV, "Terminal"),
            ("conhost.exe", "", Plain, true, false, Chord::CtrlV, "Terminal"),
            ("cmd.exe", "", Plain, true, false, Chord::CtrlV, "Terminal"),
            ("powershell.exe", "", Plain, true, false, Chord::CtrlV, "Terminal"),
            ("pwsh.exe", "", Plain, true, false, Chord::CtrlV, "Terminal"),
            ("mintty.exe", "MINGW64:/e", Plain, true, false, Chord::ShiftInsert, "Git Bash"),
            ("WINWORD.EXE", "Document1 - Word", Html, false, true, Chord::CtrlV, "Word"),
            ("outlook.exe", "Inbox", Html, false, true, Chord::CtrlV, "Outlook"),
            ("olk.exe", "Mail", Html, false, true, Chord::CtrlV, "Outlook"),
            (
                "chrome.exe",
                "Inbox (3) - me@example.com - Gmail - Google Chrome",
                Html,
                false,
                true,
                Chord::CtrlV,
                "Gmail",
            ),
            ("msedge.exe", "GMAIL", Html, false, true, Chord::CtrlV, "Gmail"),
            ("firefox.exe", "Compose - Gmail — Mozilla Firefox", Html, false, true, Chord::CtrlV, "Gmail"),
            ("chrome.exe", "Bolo - GitHub - Google Chrome", Plain, false, true, Chord::CtrlV, "chrome"),
            ("slack.exe", "general", Markdown, false, true, Chord::CtrlV, "Slack"),
            ("notion.exe", "", Markdown, false, true, Chord::CtrlV, "Notion"),
            ("obsidian.exe", "", Markdown, false, true, Chord::CtrlV, "Obsidian"),
            ("discord.exe", "", Markdown, false, true, Chord::CtrlV, "Discord"),
            ("notepad.exe", "notes.txt - Notepad", Plain, false, true, Chord::CtrlV, "notepad"),
            ("", "", Plain, false, true, Chord::CtrlV, ""),
        ];
        for &(exe, title, format, code_mode, polish, chord, name) in cases {
            let r = bundled(exe, title, &[], true);
            let got = (r.ctx.format, r.ctx.code_mode, r.polish, r.target.chord, r.app_name.as_str());
            assert_eq!(got, (format, code_mode, polish, chord, name), "{exe} / {title}");
            assert_eq!(r.target.format, format, "the inserter and the engine agree: {exe}");
            assert_eq!(r.ctx.app_id, exe.to_lowercase());
            assert_eq!(r.tone, "neutral", "no rule sets a tone: {exe}");
            assert!(!r.target.multiline_swap_safe, "nothing is multi-line-safe until checked by hand: {exe}");
        }
    }

    #[test]
    fn a_gmail_title_outside_a_browser_is_not_gmail() {
        let r = bundled("notepad.exe", "gmail notes.txt - Notepad", &[], true);
        assert_eq!((r.ctx.format, r.app_name.as_str()), (Plain, "notepad"));
    }

    #[test]
    fn only_browsers_pass_their_title_to_the_engine() {
        assert_eq!(bundled("chrome.exe", "Gmail", &[], false).ctx.window_title.as_deref(), Some("Gmail"));
        assert_eq!(bundled("chrome.exe", "", &[], false).ctx.window_title, None);
        assert_eq!(bundled("notepad.exe", "secret.txt - Notepad", &[], false).ctx.window_title, None);
    }

    #[test]
    fn polish_follows_the_global_switch_where_no_rule_turns_it_off() {
        assert!(!bundled("slack.exe", "", &[], false).polish);
        assert!(bundled("slack.exe", "", &[], true).polish);
        assert!(!bundled("code.exe", "", &[], true).polish);
    }

    #[test]
    fn user_polish_wins_over_the_table_and_the_global_switch() {
        assert!(bundled("code.exe", "", &[polish("code.exe", true)], false).polish, "on in an editor");
        assert!(!bundled("slack.exe", "", &[polish("Slack", false)], true).polish, "case and .exe don't matter");
        assert!(bundled("notepad.exe", "", &[polish("NOTEPAD.EXE", true)], false).polish);
        assert!(bundled("notepad.exe", "", &[polish("slack.exe", false)], true).polish, "other apps' rows");
        let r = bundled("code.exe", "", &[polish("code.exe", true)], false);
        assert!(r.ctx.code_mode, "a user row can't change code mode");
    }

    #[test]
    fn each_field_comes_from_the_user_then_the_rule_then_the_default() {
        let slack = AppProfile { tone: Some("casual".into()), ..AppProfile::new("slack.exe") };
        let r = bundled("slack.exe", "", &[slack], true);
        assert_eq!((r.ctx.format, r.tone.as_str(), r.app_name.as_str(), r.polish), (Markdown, "casual", "Slack", true));

        let slack = AppProfile {
            name: Some(" Work chat ".into()),
            format: Some(Plain),
            polish: Some(false),
            ..AppProfile::new("slack.exe")
        };
        let r = bundled("slack.exe", "", &[slack], true);
        assert_eq!((r.ctx.format, r.target.format), (Plain, Plain), "the user's format reaches both");
        assert_eq!((r.app_name.as_str(), r.polish, r.tone.as_str()), ("Work chat", false, "neutral"));

        let whatsapp = AppProfile { format: Some(Markdown), tone: Some(" ".into()), ..AppProfile::new("whatsapp") };
        let r =
            resolve("WhatsApp.exe", "", &AppRules::bundled(), &[whatsapp], Defaults { polish: true, tone: "formal" });
        assert_eq!((r.ctx.format, r.tone.as_str(), r.app_name.as_str()), (Markdown, "formal", "whatsapp"));
        assert_eq!(r.target.chord, Chord::CtrlV);

        let r = resolve("notepad.exe", "", &AppRules::bundled(), &[], Defaults { polish: false, tone: "  " });
        assert_eq!(r.tone, "neutral", "a blank global tone is neutral");
    }

    #[test]
    fn a_title_rule_matches_first_and_the_browser_row_fills_the_rest() {
        let chrome = AppProfile {
            name: Some("Chrome".into()),
            format: Some(Markdown),
            polish: Some(false),
            tone: Some("formal".into()),
            ..AppProfile::new("chrome.exe")
        };
        let gmail = bundled("chrome.exe", "Inbox - Gmail - Google Chrome", std::slice::from_ref(&chrome), true);
        assert_eq!((gmail.ctx.format, gmail.app_name.as_str()), (Html, "Gmail"), "the title rule's fields");
        assert_eq!((gmail.polish, gmail.tone.as_str()), (false, "formal"), "the row's, where the rule says nothing");
        let other = bundled("chrome.exe", "Bolo - GitHub", &[chrome], true);
        assert_eq!((other.ctx.format, other.app_name.as_str(), other.polish), (Markdown, "Chrome", false));
    }

    #[test]
    fn a_row_for_a_rule_covers_every_exe_the_rule_lists_unless_one_has_its_own() {
        let terminal =
            AppProfile { polish: Some(true), tone: Some("casual".into()), ..AppProfile::new("windowsterminal.exe") };
        let pwsh = AppProfile { tone: Some("formal".into()), ..AppProfile::new("pwsh.exe") };
        let rows = [terminal, pwsh];
        let r = bundled("cmd.exe", "", &rows, false);
        assert_eq!((r.polish, r.tone.as_str(), r.ctx.code_mode), (true, "casual", true));
        let r = bundled("pwsh.exe", "", &rows, false);
        assert_eq!((r.polish, r.tone.as_str()), (true, "formal"), "its own row first, field by field");
        assert!(!bundled("code.exe", "", &rows, false).polish, "other rules' apps");
    }

    #[test]
    fn the_first_matching_rule_wins_and_fields_are_optional() {
        let rules = AppRules::parse(
            r#"
            browsers = ["Chrome.exe"]
            [[rule]]
            title = "Docs"
            format = "html"
            multiline_swap_safe = true
            [[rule]]
            exe = ["chrome.exe"]
            format = "markdown"
            paste = "shift-insert"
            [[rule]]
            exe = ["chrome.exe"]
            format = "plain"
            "#,
        )
        .unwrap();
        let docs = resolve("chrome.exe", "Untitled - Google Docs", &rules, &[], defaults(false));
        assert_eq!((docs.ctx.format, docs.target.multiline_swap_safe, docs.app_name.as_str()), (Html, true, "chrome"));
        let other = resolve("chrome.exe", "Bolo", &rules, &[], defaults(false));
        assert_eq!((other.ctx.format, other.target.chord), (Markdown, Chord::ShiftInsert));
    }

    #[test]
    fn a_misspelt_field_is_an_error() {
        assert!(AppRules::parse("[[rule]]\nexe = [\"a.exe\"]\ncodemode = true").is_err());
        assert!(AppRules::parse("[[rule]]\nexe = [\"a.exe\"]\npaste = \"ctrl-shift-v\"").is_err());
    }

    #[test]
    fn the_app_rows_say_where_each_value_comes_from() {
        let rules = AppRules::bundled();
        let profiles = [
            AppProfile { format: Some(Plain), ..AppProfile::new("slack.exe") },
            AppProfile { tone: Some("casual".into()), ..AppProfile::new("WhatsApp") },
            AppProfile::new("figma.exe"),
        ];
        let rows = app_views(&rules, &profiles, Defaults { polish: true, tone: "formal" });
        let bundled_apps = rules.rules.iter().filter(|r| r.title.is_none() && !r.exe.is_empty()).count();
        assert_eq!(rows.len(), bundled_apps + 2, "Gmail, a title rule, and the Android apps have no row");

        let term = rows.iter().find(|r| r.exe == "windowsterminal.exe").unwrap();
        assert!(term.exes.contains(&"pwsh.exe".to_owned()) && term.code_mode && term.bundled && !term.custom);
        assert_eq!((term.fields.polish.value, term.fields.polish.source), (false, Source::Bundled));

        let slack = rows.iter().find(|r| r.exe == "slack.exe").unwrap();
        assert!(slack.custom && slack.bundled);
        assert_eq!((slack.fields.format.value, slack.fields.format.source), (Plain, Source::User));
        assert_eq!((slack.fields.name.value.as_str(), slack.fields.name.source), ("Slack", Source::Bundled));
        assert_eq!((slack.fields.polish.value, slack.fields.polish.source), (true, Source::Default));
        assert_eq!((slack.fields.tone.value.as_str(), slack.fields.tone.source), ("formal", Source::Default));

        let whatsapp = &rows[bundled_apps];
        assert_eq!((whatsapp.exe.as_str(), whatsapp.bundled, whatsapp.custom), ("whatsapp.exe", false, true));
        assert_eq!((whatsapp.fields.tone.value.as_str(), whatsapp.fields.tone.source), ("casual", Source::User));
        assert_eq!((whatsapp.fields.name.value.as_str(), whatsapp.fields.name.source), ("whatsapp", Source::Default));
        assert_eq!(slack.inherited.format.value, Markdown, "clearing the format goes back to the rule's");
        let figma = &rows[bundled_apps + 1];
        assert_eq!((figma.exe.as_str(), figma.custom), ("figma.exe", false), "added, nothing set yet");

        let json = serde_json::to_value(slack).unwrap();
        assert_eq!(json["format"], serde_json::json!({ "value": "plain", "source": "user" }));
        assert_eq!(json["polish"]["source"], "default");
    }

    #[test]
    fn the_plan_turns_polish_on_with_the_app_name_and_tone_only_with_the_key() {
        let mut prefs = PolishPrefs {
            enabled: true,
            tone: " ".into(),
            dictionary: vec!["Bolo".into()],
            has_key: true,
            ..Default::default()
        };
        let rules = AppRules::bundled();
        let p = plan("chrome.exe", "Gmail", &rules, &prefs);
        assert_eq!(
            p.polish,
            Polish::On { tone: "neutral".into(), app: "Gmail".into(), dictionary: vec!["Bolo".into()] }
        );
        assert_eq!((p.ctx.format, p.target.format), (Html, Html));
        assert_eq!(plan("code.exe", "", &rules, &prefs).polish, Polish::Off);
        prefs.apps = vec![AppProfile { tone: Some("casual".into()), ..AppProfile::new("slack.exe") }];
        let Polish::On { tone, app, .. } = plan("slack.exe", "", &rules, &prefs).polish else { panic!() };
        assert_eq!((tone.as_str(), app.as_str()), ("casual", "Slack"));
        prefs.has_key = false;
        assert_eq!(plan("chrome.exe", "Gmail", &rules, &prefs).polish, Polish::Off, "no key, no polish");
    }
}
