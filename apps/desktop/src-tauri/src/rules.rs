//! App rules: the window in front decides how a dictation's text goes in.
//!
//! The bundled table (`app_rules.toml`, documented there) maps the foreground exe, and
//! for browsers the window title, to a [`Resolved`] target: the format and `code_mode`
//! the engine formats for, the paste chord and multi-line swap safety the inserter uses,
//! and whether polish runs. The user's per-exe polish choices in settings win over the
//! table. [`resolve`] is pure, so the table is tested without a desktop.

use std::collections::BTreeMap;

use bolo_core::{OutputFormat, Polish, TargetContext};
use serde::Deserialize;

use crate::insert::{Chord, Target};

/// The table bundled into the app.
pub const BUNDLED: &str = include_str!("../app_rules.toml");

/// One `[[rule]]` from the table.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    /// The app's name for polish, e.g. "Gmail".
    pub name: Option<String>,
    /// Lower-case image names. Empty: the rule is for browsers, by `title`.
    #[serde(default)]
    pub exe: Vec<String>,
    /// Matched case-insensitively as a substring of the window title.
    pub title: Option<String>,
    pub format: Option<OutputFormat>,
    #[serde(default)]
    pub code_mode: bool,
    /// `Some(false)` turns polish off in this app; `None` follows the global setting.
    pub polish: Option<bool>,
    pub paste: Option<Paste>,
    #[serde(default)]
    pub multiline_swap_safe: bool,
}

/// The `paste` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Paste {
    CtrlV,
    ShiftInsert,
}

impl From<Paste> for Chord {
    fn from(p: Paste) -> Self {
        match p {
            Paste::CtrlV => Chord::CtrlV,
            Paste::ShiftInsert => Chord::ShiftInsert,
        }
    }
}

/// The whole table.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppRules {
    /// Browsers' exe names; their window title is read and title rules apply to them.
    #[serde(default)]
    pub browsers: Vec<String>,
    #[serde(default, rename = "rule")]
    pub rules: Vec<Rule>,
}

impl AppRules {
    pub fn parse(text: &str) -> Result<Self, toml::de::Error> {
        let mut rules: Self = toml::from_str(text)?;
        let lower = |names: &mut Vec<String>| names.iter_mut().for_each(|n| *n = n.trim().to_lowercase());
        lower(&mut rules.browsers);
        for rule in &mut rules.rules {
            lower(&mut rule.exe);
            rule.title = rule.title.take().map(|t| t.to_lowercase());
        }
        Ok(rules)
    }

    /// The bundled table. It's checked by a test, so this can't fail in a release.
    pub fn bundled() -> Self {
        Self::parse(BUNDLED).expect("the bundled app_rules.toml parses")
    }

    fn is_browser(&self, exe: &str) -> bool {
        self.browsers.iter().any(|b| b == exe)
    }

    /// The first rule for `exe` (lower-case) with window title `title`.
    fn find(&self, exe: &str, title: &str) -> Option<&Rule> {
        let browser = self.is_browser(exe);
        let title = title.to_lowercase();
        self.rules.iter().find(|r| {
            let exe_ok = if r.exe.is_empty() { browser && r.title.is_some() } else { r.exe.iter().any(|e| e == exe) };
            exe_ok && r.title.as_ref().is_none_or(|t| title.contains(t.as_str()))
        })
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
    /// The app's name for polish: the rule's name, else the exe without `.exe`.
    pub app_name: String,
}

/// The target for the window in front: `exe` is its image name, `title` its title.
/// `overrides` are the user's per-exe polish choices (settings `[polish.apps]`), and
/// `polish_default` the global polish switch.
pub fn resolve(
    exe: &str,
    title: &str,
    rules: &AppRules,
    overrides: &BTreeMap<String, bool>,
    polish_default: bool,
) -> Resolved {
    let exe = exe.trim().to_lowercase();
    let rule = rules.find(&exe, title);
    let format = rule.and_then(|r| r.format).unwrap_or_default();
    let polish = override_for(&exe, overrides).unwrap_or_else(|| rule.and_then(|r| r.polish).unwrap_or(polish_default));
    let app_name =
        rule.and_then(|r| r.name.clone()).unwrap_or_else(|| exe.strip_suffix(".exe").unwrap_or(&exe).to_owned());
    Resolved {
        ctx: TargetContext {
            window_title: (rules.is_browser(&exe) && !title.is_empty()).then(|| title.to_owned()),
            app_id: exe,
            format,
            code_mode: rule.is_some_and(|r| r.code_mode),
        },
        target: Target {
            format,
            chord: rule.and_then(|r| r.paste).map(Chord::from).unwrap_or_default(),
            multiline_swap_safe: rule.is_some_and(|r| r.multiline_swap_safe),
        },
        polish,
        app_name,
    }
}

/// The user's polish settings, as far as the app rules need them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PolishPrefs {
    /// The global switch.
    pub enabled: bool,
    pub tone: String,
    pub dictionary: Vec<String>,
    /// Per-exe on/off (settings `[polish.apps]`); wins over the table and the switch.
    pub apps: BTreeMap<String, bool>,
    /// The Anthropic key is stored. Without it polish never runs.
    pub has_key: bool,
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
    let r = resolve(exe, title, rules, &prefs.apps, prefs.enabled);
    let polish = if r.polish && prefs.has_key {
        let tone = prefs.tone.trim();
        Polish::On {
            tone: if tone.is_empty() { "neutral" } else { tone }.into(),
            app: r.app_name,
            dictionary: prefs.dictionary.clone(),
        }
    } else {
        Polish::Off
    };
    Plan { ctx: r.ctx, polish, target: r.target }
}

/// The user's choice for `exe`. Keys are matched ignoring case, with or without `.exe`.
fn override_for(exe: &str, overrides: &BTreeMap<String, bool>) -> Option<bool> {
    overrides.iter().find_map(|(k, &on)| {
        let k = k.trim().to_lowercase();
        let k = if k.ends_with(".exe") { k } else { format!("{k}.exe") };
        (k == exe).then_some(on)
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use bolo_core::OutputFormat::{self, Html, Markdown, Plain};
    use bolo_core::Polish;

    use super::{AppRules, PolishPrefs, Resolved, plan, resolve};
    use crate::insert::Chord;

    fn bundled(exe: &str, title: &str, overrides: &[(&str, bool)], polish_default: bool) -> Resolved {
        let overrides: BTreeMap<String, bool> = overrides.iter().map(|(k, v)| ((*k).into(), *v)).collect();
        resolve(exe, title, &AppRules::bundled(), &overrides, polish_default)
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
    fn user_overrides_win_over_the_table_and_the_global_switch() {
        assert!(bundled("code.exe", "", &[("code.exe", true)], false).polish, "on in an editor");
        assert!(!bundled("slack.exe", "", &[("Slack", false)], true).polish, "case and .exe don't matter");
        assert!(bundled("notepad.exe", "", &[("NOTEPAD.EXE", true)], false).polish);
        assert!(bundled("notepad.exe", "", &[("slack.exe", false)], true).polish, "other apps' overrides");
        let r = bundled("code.exe", "", &[("code.exe", true)], false);
        assert!(r.ctx.code_mode, "an override changes polish only");
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
        let none = BTreeMap::new();
        let docs = resolve("chrome.exe", "Untitled - Google Docs", &rules, &none, false);
        assert_eq!((docs.ctx.format, docs.target.multiline_swap_safe, docs.app_name.as_str()), (Html, true, "chrome"));
        let other = resolve("chrome.exe", "Bolo", &rules, &none, false);
        assert_eq!((other.ctx.format, other.target.chord), (Markdown, Chord::ShiftInsert));
    }

    #[test]
    fn a_misspelt_field_is_an_error() {
        assert!(AppRules::parse("[[rule]]\nexe = [\"a.exe\"]\ncodemode = true").is_err());
        assert!(AppRules::parse("[[rule]]\nexe = [\"a.exe\"]\npaste = \"ctrl-shift-v\"").is_err());
    }

    #[test]
    fn the_plan_turns_polish_on_with_the_app_name_only_with_the_key() {
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
        prefs.has_key = false;
        assert_eq!(plan("chrome.exe", "Gmail", &rules, &prefs).polish, Polish::Off, "no key, no polish");
    }
}
