//! App rules: the window in front decides how a dictation's text goes in.
//!
//! The bundled table (`app_rules.toml`, documented there) maps the foreground exe, and
//! for browsers the window title, to a [`Resolved`] target: the format and `code_mode`
//! the engine formats for, the paste chord and multi-line swap safety the inserter uses,
//! and whether polish runs. The user's `[[apps]]` rows in settings (ADR-0020) win over
//! the table field by field for the name, polish, tone and format; code mode, the paste
//! chord and swap safety are the table's alone. [`resolve`] is pure, so the table is
//! tested without a desktop.

use bolo_core::{OutputFormat, Polish, TargetContext};
use bolo_store::{AppProfile, exe_name};
use serde::{Deserialize, Serialize};

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

/// Where a field's value came from: the user's `[[apps]]` row, the bundled rule, or the
/// default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    User,
    Bundled,
    Default,
}

/// A field's value and where it came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Field<T> {
    pub value: T,
    pub source: Source,
}

/// The defaults for an app that neither the user nor the bundled rules decide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Defaults<'a> {
    /// The global polish switch.
    pub polish: bool,
    /// The global tone; blank means neutral.
    pub tone: &'a str,
}

/// The fields a user profile can set, resolved for one app.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Fields {
    /// The app's name for polish and the window.
    pub name: Field<String>,
    pub polish: Field<bool>,
    pub tone: Field<String>,
    pub format: Field<OutputFormat>,
}

/// The user's rows for `exe`, most specific first: its own row, then the row of the
/// first exe of the bundled rule that lists it (the Apps tab shows one row for, say,
/// every terminal, stored under the first one).
fn profiles_for<'a>(exe: &str, rules: &AppRules, profiles: &'a [AppProfile]) -> Vec<&'a AppProfile> {
    let by = |name: &str| profiles.iter().find(|p| exe_name(&p.exe) == name);
    let group = rules.rules.iter().find(|r| r.exe.iter().any(|e| e == exe)).map(|r| r.exe[0].as_str());
    by(exe).into_iter().chain(group.filter(|g| *g != exe).and_then(by)).collect()
}

/// `exe`'s fields under `rule`. Each comes from the user's rows, then the rule, then
/// the default; for a browser tab matched by a title rule (Gmail), the rule comes first.
fn fields(exe: &str, rule: Option<&Rule>, user: &[&AppProfile], defaults: Defaults) -> Fields {
    let title_rule = rule.is_some_and(|r| r.title.is_some());
    fn pick<T>(user: Option<T>, bundled: Option<T>, title_rule: bool, default: T) -> Field<T> {
        let (user, bundled) = ((user, Source::User), (bundled, Source::Bundled));
        let (first, second) = if title_rule { (bundled, user) } else { (user, bundled) };
        match (first, second) {
            ((Some(value), source), _) | (_, (Some(value), source)) => Field { value, source },
            _ => Field { value: default, source: Source::Default },
        }
    }
    let text = |s: &Option<String>| s.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(String::from);
    let tone = defaults.tone.trim();
    Fields {
        name: pick(
            user.iter().find_map(|p| text(&p.name)),
            rule.and_then(|r| r.name.clone()),
            title_rule,
            exe.strip_suffix(".exe").unwrap_or(exe).to_owned(),
        ),
        polish: pick(user.iter().find_map(|p| p.polish), rule.and_then(|r| r.polish), title_rule, defaults.polish),
        tone: pick(
            user.iter().find_map(|p| text(&p.tone)),
            None,
            title_rule,
            if tone.is_empty() { "neutral" } else { tone }.to_owned(),
        ),
        format: pick(
            user.iter().find_map(|p| p.format),
            rule.and_then(|r| r.format),
            title_rule,
            OutputFormat::default(),
        ),
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

/// The target for the window in front: `exe` is its image name, `title` its title.
/// `profiles` are the user's `[[apps]]` rows; they win over the bundled rules field by
/// field (ADR-0020). Code mode, the paste chord and swap safety are the rules' alone.
pub fn resolve(exe: &str, title: &str, rules: &AppRules, profiles: &[AppProfile], defaults: Defaults) -> Resolved {
    let exe = exe.trim().to_lowercase();
    let rule = rules.find(&exe, title);
    let f = fields(&exe, rule, &profiles_for(&exe, rules, profiles), defaults);
    Resolved {
        ctx: TargetContext {
            window_title: (rules.is_browser(&exe) && !title.is_empty()).then(|| title.to_owned()),
            app_id: exe,
            format: f.format.value,
            code_mode: rule.is_some_and(|r| r.code_mode),
        },
        target: Target {
            format: f.format.value,
            chord: rule.and_then(|r| r.paste).map(Chord::from).unwrap_or_default(),
            multiline_swap_safe: rule.is_some_and(|r| r.multiline_swap_safe),
        },
        polish: f.polish.value,
        tone: f.tone.value,
        app_name: f.name.value,
    }
}

/// One row of the Apps tab: an app the bundled rules know or the user added.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AppView {
    /// The exe the user's row is stored under.
    pub exe: String,
    /// Every exe the row covers (a bundled rule may list several).
    pub exes: Vec<String>,
    #[serde(flatten)]
    pub fields: Fields,
    /// The fields without the user's row: what clearing a field goes back to.
    pub inherited: Fields,
    /// Bundled: code mode, which the user can't change.
    pub code_mode: bool,
    /// The bundled rules know this app; resetting it keeps it listed.
    pub bundled: bool,
    /// The user set at least one field here.
    pub custom: bool,
}

/// The Apps tab's rows: the bundled rules' apps in table order, then the apps only the
/// user's rows name, in the order they were added.
pub fn app_views(rules: &AppRules, profiles: &[AppProfile], defaults: Defaults) -> Vec<AppView> {
    let row = |exe: &str, exes: Vec<String>, bundled: bool| {
        let rule = rules.rules.iter().find(|r| r.title.is_none() && r.exe.iter().any(|e| e == exe));
        let user = profiles_for(exe, rules, profiles);
        let custom = user.first().is_some_and(|p| exe_name(&p.exe) == exe && is_set(p));
        let inherited: Vec<&AppProfile> = user.iter().copied().filter(|p| exe_name(&p.exe) != exe).collect();
        AppView {
            exe: exe.to_owned(),
            exes,
            fields: fields(exe, rule, &user, defaults),
            inherited: fields(exe, rule, &inherited, defaults),
            code_mode: rule.is_some_and(|r| r.code_mode),
            bundled,
            custom,
        }
    };
    let mut rows: Vec<AppView> = rules
        .rules
        .iter()
        .filter(|r| r.title.is_none() && !r.exe.is_empty())
        .map(|r| row(&r.exe[0], r.exe.clone(), true))
        .collect();
    for p in profiles {
        let exe = exe_name(&p.exe);
        if exe != ".exe" && !rows.iter().any(|r| r.exe == exe) {
            rows.push(row(&exe, vec![exe.clone()], false));
        }
    }
    rows
}

/// The row sets at least one field.
fn is_set(p: &AppProfile) -> bool {
    p.name.is_some() || p.polish.is_some() || p.tone.is_some() || p.format.is_some()
}

/// The user's polish settings, as far as the app rules need them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PolishPrefs {
    /// The global switch.
    pub enabled: bool,
    /// The global tone.
    pub tone: String,
    pub dictionary: Vec<String>,
    /// The user's `[[apps]]` rows; they win over the table, the switch and the tone.
    pub apps: Vec<AppProfile>,
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
    let defaults = Defaults { polish: prefs.enabled, tone: &prefs.tone };
    let r = resolve(exe, title, rules, &prefs.apps, defaults);
    let polish = if r.polish && prefs.has_key {
        Polish::On { tone: r.tone, app: r.app_name, dictionary: prefs.dictionary.clone() }
    } else {
        Polish::Off
    };
    Plan { ctx: r.ctx, polish, target: r.target }
}

#[cfg(test)]
mod tests {
    use bolo_core::OutputFormat::{self, Html, Markdown, Plain};
    use bolo_core::Polish;
    use bolo_store::AppProfile;

    use super::{AppRules, Defaults, PolishPrefs, Resolved, Source, app_views, plan, resolve};
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
        let bundled_apps = rules.rules.iter().filter(|r| r.title.is_none()).count();
        assert_eq!(rows.len(), bundled_apps + 2, "Gmail, a title rule, has no row");

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
