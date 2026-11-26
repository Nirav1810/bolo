//! App rules and app profiles: the app the text goes into decides how a dictation's text
//! is formatted and polished. Shared by the desktop app (by exe and window title) and the
//! Android keyboard (by package name, ADR-0022).
//!
//! The bundled table (`app_rules.toml`, documented there) maps an [`AppKey`], and on
//! Windows the browser's window title, to a [`Resolved`] target: the format and
//! `code_mode` the engine formats for, whether polish runs, and the desktop inserter's
//! paste chord and multi-line swap safety. The user's `[[apps]]` rows in settings
//! (ADR-0020) win over the table field by field for the name, polish, tone and format;
//! code mode, the paste chord and swap safety are the table's alone. Everything here is
//! pure, so the table is tested without a desktop or a phone.

use bolo_core::{OutputFormat, Polish, TargetContext};
use serde::{Deserialize, Serialize};

use crate::settings::{AppProfile, package_name};

/// The table bundled into Bolo.
pub const BUNDLED: &str = include_str!("../app_rules.toml");

/// Which kind of app a key names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppKind {
    /// A Windows program, by its image name.
    Exe,
    /// An Android app, by its package name.
    Package,
}

/// The app a dictation goes into.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AppKey {
    /// A Windows image name, trimmed and lower-cased (`slack.exe`).
    Exe(String),
    /// An Android package name, trimmed and lower-cased (`com.whatsapp`).
    Package(String),
}

impl AppKey {
    /// The foreground window's exe, as the desktop reads it: trimmed and lower-cased.
    pub fn exe(exe: &str) -> Self {
        Self::Exe(exe.trim().to_lowercase())
    }

    /// A text field's package (`EditorInfo.packageName`), tidied by [`package_name`].
    pub fn package(package: &str) -> Self {
        Self::Package(package_name(package))
    }

    pub fn kind(&self) -> AppKind {
        match self {
            Self::Exe(_) => AppKind::Exe,
            Self::Package(_) => AppKind::Package,
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Exe(s) | Self::Package(s) => s,
        }
    }

    fn of_kind(kind: AppKind, name: &str) -> Self {
        match kind {
            AppKind::Exe => Self::Exe(name.to_owned()),
            AppKind::Package => Self::Package(name.to_owned()),
        }
    }

    /// The name polish gets when nobody named the app: the exe without `.exe`, or the
    /// package's last part (`com.example.notes` → `notes`).
    fn default_name(&self) -> String {
        match self {
            Self::Exe(e) => e.strip_suffix(".exe").unwrap_or(e).to_owned(),
            Self::Package(p) => p.rsplit('.').next().unwrap_or(p).to_owned(),
        }
    }
}

/// One `[[rule]]` from the table.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    /// The app's name for polish, e.g. "Gmail".
    pub name: Option<String>,
    /// Lower-case image names. Empty with a `title`: the rule is for browsers, by title.
    #[serde(default)]
    pub exe: Vec<String>,
    /// Lower-case Android package names.
    #[serde(default)]
    pub package: Vec<String>,
    /// Matched case-insensitively as a substring of the window title.
    pub title: Option<String>,
    pub format: Option<OutputFormat>,
    #[serde(default)]
    pub code_mode: bool,
    /// `Some(false)` turns polish off in this app; `None` follows the global setting.
    pub polish: Option<bool>,
    /// The desktop's paste chord.
    pub paste: Option<Paste>,
    /// The desktop's polish swap may select back multi-line text here.
    #[serde(default)]
    pub multiline_swap_safe: bool,
}

impl Rule {
    /// The names this rule lists for apps of `kind`.
    pub fn keys(&self, kind: AppKind) -> &[String] {
        match kind {
            AppKind::Exe => &self.exe,
            AppKind::Package => &self.package,
        }
    }

    fn lists(&self, app: &AppKey) -> bool {
        self.keys(app.kind()).iter().any(|k| k == app.as_str())
    }
}

/// The `paste` field: which keys paste on the desktop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Paste {
    CtrlV,
    ShiftInsert,
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
            lower(&mut rule.package);
            rule.title = rule.title.take().map(|t| t.to_lowercase());
        }
        Ok(rules)
    }

    /// The bundled table. It's checked by a test, so this can't fail in a release.
    pub fn bundled() -> Self {
        Self::parse(BUNDLED).expect("the bundled app_rules.toml parses")
    }

    fn is_browser(&self, app: &AppKey) -> bool {
        matches!(app, AppKey::Exe(exe) if self.browsers.iter().any(|b| b == exe))
    }

    /// The first rule for `app` with window title `title`.
    fn find(&self, app: &AppKey, title: &str) -> Option<&Rule> {
        let browser = self.is_browser(app);
        let title = title.to_lowercase();
        self.rules.iter().find(|r| {
            let by_title = r.exe.is_empty() && r.package.is_empty();
            let app_ok = if by_title { browser && r.title.is_some() } else { r.lists(app) };
            app_ok && r.title.as_ref().is_none_or(|t| title.contains(t.as_str()))
        })
    }

    /// The rule without a title that lists `app` (the Apps tab's row for it).
    fn listing(&self, app: &AppKey) -> Option<&Rule> {
        self.rules.iter().find(|r| r.title.is_none() && r.lists(app))
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
    /// The app's name for polish and the settings window.
    pub name: Field<String>,
    pub polish: Field<bool>,
    pub tone: Field<String>,
    pub format: Field<OutputFormat>,
}

/// The user's rows for `app`, most specific first: its own row, then the row of the
/// first app of the bundled rule that lists it (the Apps tab shows one row for, say,
/// every terminal, stored under the first one).
fn profiles_for<'a>(app: &AppKey, rules: &AppRules, profiles: &'a [AppProfile]) -> Vec<&'a AppProfile> {
    let by = |key: &AppKey| profiles.iter().find(|p| p.matches(key));
    let group = rules
        .rules
        .iter()
        .find(|r| r.lists(app))
        .and_then(|r| r.keys(app.kind()).first())
        .map(|first| AppKey::of_kind(app.kind(), first));
    by(app).into_iter().chain(group.filter(|g| g != app).as_ref().and_then(by)).collect()
}

/// `app`'s fields under `rule`. Each comes from the user's rows, then the rule, then the
/// default; for a browser tab matched by a title rule (Gmail), the rule comes first.
fn fields(app: &AppKey, rule: Option<&Rule>, user: &[&AppProfile], defaults: Defaults) -> Fields {
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
            app.default_name(),
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

/// What Bolo does for a dictation into one app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    /// For the engine: the app, the title (desktop browsers only), the format and
    /// `code_mode`.
    pub ctx: TargetContext,
    /// Whether polish should run here. It still needs the Anthropic key.
    pub polish: bool,
    /// Polish's tone here.
    pub tone: String,
    /// The app's name for polish: the user's or the rule's name, else
    /// [`AppKey::default_name`].
    pub app_name: String,
    /// The rule's paste chord (desktop).
    pub paste: Option<Paste>,
    /// The rule allows a multi-line polish swap (desktop).
    pub multiline_swap_safe: bool,
}

/// The target for `app`; `title` is the window title (desktop; Android passes "").
/// `profiles` are the user's `[[apps]]` rows; they win over the bundled rules field by
/// field (ADR-0020). Code mode, the paste chord and swap safety are the rules' alone.
pub fn resolve(app: &AppKey, title: &str, rules: &AppRules, profiles: &[AppProfile], defaults: Defaults) -> Resolved {
    let rule = rules.find(app, title);
    let f = fields(app, rule, &profiles_for(app, rules, profiles), defaults);
    Resolved {
        ctx: TargetContext {
            window_title: (rules.is_browser(app) && !title.is_empty()).then(|| title.to_owned()),
            app_id: app.as_str().to_owned(),
            format: f.format.value,
            code_mode: rule.is_some_and(|r| r.code_mode),
        },
        polish: f.polish.value,
        tone: f.tone.value,
        app_name: f.name.value,
        paste: rule.and_then(|r| r.paste),
        multiline_swap_safe: rule.is_some_and(|r| r.multiline_swap_safe),
    }
}

/// One row of the Apps tab: an app the bundled rules know or the user added.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AppView {
    /// The exe (desktop) or package (Android) the user's row is stored under.
    pub exe: String,
    /// Every exe or package the row covers (a bundled rule may list several).
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

/// The Apps tab's rows for apps of `kind`: the bundled rules' apps in table order, then
/// the apps only the user's rows name, in the order they were added.
pub fn app_views(kind: AppKind, rules: &AppRules, profiles: &[AppProfile], defaults: Defaults) -> Vec<AppView> {
    let row = |app: &AppKey, names: Vec<String>, bundled: bool| {
        let rule = rules.listing(app);
        let user = profiles_for(app, rules, profiles);
        let custom = user.first().is_some_and(|p| p.matches(app) && p.is_set());
        let inherited: Vec<&AppProfile> = user.iter().copied().filter(|p| !p.matches(app)).collect();
        AppView {
            exe: app.as_str().to_owned(),
            exes: names,
            fields: fields(app, rule, &user, defaults),
            inherited: fields(app, rule, &inherited, defaults),
            code_mode: rule.is_some_and(|r| r.code_mode),
            bundled,
            custom,
        }
    };
    let mut rows: Vec<AppView> = rules
        .rules
        .iter()
        .filter(|r| r.title.is_none() && !r.keys(kind).is_empty())
        .map(|r| row(&AppKey::of_kind(kind, &r.keys(kind)[0]), r.keys(kind).to_vec(), true))
        .collect();
    for p in profiles {
        let Some(app) = p.key(kind) else { continue };
        if !rows.iter().any(|r| r.exe == app.as_str()) {
            rows.push(row(&app, vec![app.as_str().to_owned()], false));
        }
    }
    rows
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

/// A dictation into one app: where it goes and the engine's polish config for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub resolved: Resolved,
    pub polish: Polish,
}

/// The [`Plan`] for `app` (with window title `title` on the desktop).
pub fn plan(app: &AppKey, title: &str, rules: &AppRules, prefs: &PolishPrefs) -> Plan {
    let defaults = Defaults { polish: prefs.enabled, tone: &prefs.tone };
    let r = resolve(app, title, rules, &prefs.apps, defaults);
    let polish = if r.polish && prefs.has_key {
        Polish::On { tone: r.tone.clone(), app: r.app_name.clone(), dictionary: prefs.dictionary.clone() }
    } else {
        Polish::Off
    };
    Plan { resolved: r, polish }
}

#[cfg(test)]
mod tests {
    use bolo_core::OutputFormat::{self, Html, Markdown, Plain};
    use bolo_core::Polish;

    use super::*;

    fn defaults(polish: bool) -> Defaults<'static> {
        Defaults { polish, tone: "neutral" }
    }

    fn exe(exe: &str, title: &str, profiles: &[AppProfile], polish_default: bool) -> Resolved {
        resolve(&AppKey::exe(exe), title, &AppRules::bundled(), profiles, defaults(polish_default))
    }

    fn package(package: &str, profiles: &[AppProfile], polish_default: bool) -> Resolved {
        resolve(&AppKey::package(package), "", &AppRules::bundled(), profiles, defaults(polish_default))
    }

    /// A user row for `exe` with only polish set.
    fn polish(exe: &str, on: bool) -> AppProfile {
        AppProfile { polish: Some(on), ..AppProfile::new(exe) }
    }

    /// (exe, title) → (format, code_mode, polish with the global switch on, paste, name).
    #[test]
    fn the_bundled_desktop_table() {
        use Paste::ShiftInsert;
        type Case<'a> = (&'a str, &'a str, OutputFormat, bool, bool, Option<Paste>, &'a str);
        let cases: &[Case] = &[
            ("code.exe", "main.rs - bolo - Visual Studio Code", Plain, true, false, None, "VS Code"),
            ("Cursor.exe", "x", Plain, true, false, None, "Cursor"),
            ("idea64.exe", "", Plain, true, false, None, "IntelliJ IDEA"),
            ("devenv.exe", "", Plain, true, false, None, "Visual Studio"),
            ("WindowsTerminal.exe", "pwsh", Plain, true, false, None, "Terminal"),
            ("wezterm-gui.exe", "", Plain, true, false, None, "Terminal"),
            ("alacritty.exe", "", Plain, true, false, None, "Terminal"),
            ("conhost.exe", "", Plain, true, false, None, "Terminal"),
            ("cmd.exe", "", Plain, true, false, None, "Terminal"),
            ("powershell.exe", "", Plain, true, false, None, "Terminal"),
            ("pwsh.exe", "", Plain, true, false, None, "Terminal"),
            ("mintty.exe", "MINGW64:/e", Plain, true, false, Some(ShiftInsert), "Git Bash"),
            ("WINWORD.EXE", "Document1 - Word", Html, false, true, None, "Word"),
            ("outlook.exe", "Inbox", Html, false, true, None, "Outlook"),
            ("olk.exe", "Mail", Html, false, true, None, "Outlook"),
            ("chrome.exe", "Inbox (3) - me@example.com - Gmail - Google Chrome", Html, false, true, None, "Gmail"),
            ("msedge.exe", "GMAIL", Html, false, true, None, "Gmail"),
            ("firefox.exe", "Compose - Gmail — Mozilla Firefox", Html, false, true, None, "Gmail"),
            ("chrome.exe", "Bolo - GitHub - Google Chrome", Plain, false, true, None, "chrome"),
            ("slack.exe", "general", Markdown, false, true, None, "Slack"),
            ("notion.exe", "", Markdown, false, true, None, "Notion"),
            ("obsidian.exe", "", Markdown, false, true, None, "Obsidian"),
            ("discord.exe", "", Markdown, false, true, None, "Discord"),
            ("notepad.exe", "notes.txt - Notepad", Plain, false, true, None, "notepad"),
            ("", "", Plain, false, true, None, ""),
        ];
        for &(name, title, format, code_mode, polish, paste, app_name) in cases {
            let r = exe(name, title, &[], true);
            let got = (r.ctx.format, r.ctx.code_mode, r.polish, r.paste, r.app_name.as_str());
            assert_eq!(got, (format, code_mode, polish, paste, app_name), "{name} / {title}");
            assert_eq!(r.ctx.app_id, name.to_lowercase());
            assert_eq!(r.tone, "neutral", "no rule sets a tone: {name}");
            assert!(!r.multiline_swap_safe, "nothing is multi-line-safe until checked by hand: {name}");
        }
    }

    /// package → (format, polish with the global switch on, name).
    #[test]
    fn the_bundled_android_table() {
        let cases: &[(&str, OutputFormat, &str)] = &[
            ("com.whatsapp", Plain, "WhatsApp"),
            ("com.whatsapp.w4b", Plain, "WhatsApp"),
            ("com.google.android.gm", Plain, "Gmail"),
            ("com.android.chrome", Plain, "Chrome"),
            ("com.google.android.keep", Plain, "Keep"),
            ("com.google.android.apps.messaging", Plain, "Messages"),
            ("com.Slack", Markdown, "Slack"),
            (" com.example.Notes ", Plain, "notes"),
        ];
        for &(name, format, app_name) in cases {
            let r = package(name, &[], true);
            assert_eq!((r.ctx.format, r.polish, r.app_name.as_str()), (format, true, app_name), "{name}");
            assert_eq!(r.ctx.app_id, name.trim().to_lowercase());
            assert!(!r.ctx.code_mode && r.ctx.window_title.is_none() && r.paste.is_none(), "{name}");
        }
    }

    #[test]
    fn exes_and_packages_never_match_each_other() {
        // A package named like an exe rule's app gets no desktop rule, and the reverse.
        assert_eq!(package("slack.exe", &[], true).app_name, "exe");
        assert_eq!(exe("com.slack", "", &[], true).app_name, "com.slack");
        let rows = [polish("com.whatsapp", false)];
        assert!(package("com.whatsapp", &rows, true).polish, "an exe row (com.whatsapp.exe) isn't a package row");
        let rows = [AppProfile { polish: Some(false), ..AppProfile::for_app(&AppKey::package("com.whatsapp")) }];
        assert!(!package("com.whatsapp", &rows, true).polish);
        assert!(exe("whatsapp.exe", "", &rows, true).polish);
    }

    #[test]
    fn a_package_row_wins_field_by_field_and_covers_the_rules_other_packages() {
        let whatsapp = AppProfile {
            tone: Some("casual".into()),
            format: Some(Markdown),
            ..AppProfile::for_app(&AppKey::package("com.whatsapp"))
        };
        let r = package("com.whatsapp.w4b", &[whatsapp], true);
        assert_eq!((r.ctx.format, r.tone.as_str(), r.app_name.as_str()), (Markdown, "casual", "WhatsApp"));
    }

    #[test]
    fn a_gmail_title_outside_a_browser_is_not_gmail() {
        let r = exe("notepad.exe", "gmail notes.txt - Notepad", &[], true);
        assert_eq!((r.ctx.format, r.app_name.as_str()), (Plain, "notepad"));
    }

    #[test]
    fn only_browsers_pass_their_title_to_the_engine() {
        assert_eq!(exe("chrome.exe", "Gmail", &[], false).ctx.window_title.as_deref(), Some("Gmail"));
        assert_eq!(exe("chrome.exe", "", &[], false).ctx.window_title, None);
        assert_eq!(exe("notepad.exe", "secret.txt - Notepad", &[], false).ctx.window_title, None);
    }

    #[test]
    fn polish_follows_the_global_switch_where_no_rule_turns_it_off() {
        assert!(!exe("slack.exe", "", &[], false).polish);
        assert!(exe("slack.exe", "", &[], true).polish);
        assert!(!exe("code.exe", "", &[], true).polish);
    }

    #[test]
    fn user_polish_wins_over_the_table_and_the_global_switch() {
        assert!(exe("code.exe", "", &[polish("code.exe", true)], false).polish, "on in an editor");
        assert!(!exe("slack.exe", "", &[polish("Slack", false)], true).polish, "case and .exe don't matter");
        assert!(exe("notepad.exe", "", &[polish("NOTEPAD.EXE", true)], false).polish);
        assert!(exe("notepad.exe", "", &[polish("slack.exe", false)], true).polish, "other apps' rows");
        assert!(exe("code.exe", "", &[polish("code.exe", true)], false).ctx.code_mode, "a row can't change code mode");
    }

    #[test]
    fn each_field_comes_from_the_user_then_the_rule_then_the_default() {
        let slack = AppProfile { tone: Some("casual".into()), ..AppProfile::new("slack.exe") };
        let r = exe("slack.exe", "", &[slack], true);
        assert_eq!((r.ctx.format, r.tone.as_str(), r.app_name.as_str(), r.polish), (Markdown, "casual", "Slack", true));

        let slack = AppProfile {
            name: Some(" Work chat ".into()),
            format: Some(Plain),
            polish: Some(false),
            ..AppProfile::new("slack.exe")
        };
        let r = exe("slack.exe", "", &[slack], true);
        assert_eq!(r.ctx.format, Plain);
        assert_eq!((r.app_name.as_str(), r.polish, r.tone.as_str()), ("Work chat", false, "neutral"));

        let whatsapp = AppProfile { format: Some(Markdown), tone: Some(" ".into()), ..AppProfile::new("whatsapp") };
        let r = resolve(
            &AppKey::exe("WhatsApp.exe"),
            "",
            &AppRules::bundled(),
            &[whatsapp],
            Defaults { polish: true, tone: "formal" },
        );
        assert_eq!((r.ctx.format, r.tone.as_str(), r.app_name.as_str()), (Markdown, "formal", "whatsapp"));

        let r =
            resolve(&AppKey::exe("notepad.exe"), "", &AppRules::bundled(), &[], Defaults { polish: false, tone: "  " });
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
        let gmail = exe("chrome.exe", "Inbox - Gmail - Google Chrome", std::slice::from_ref(&chrome), true);
        assert_eq!((gmail.ctx.format, gmail.app_name.as_str()), (Html, "Gmail"), "the title rule's fields");
        assert_eq!((gmail.polish, gmail.tone.as_str()), (false, "formal"), "the row's, where the rule says nothing");
        let other = exe("chrome.exe", "Bolo - GitHub", &[chrome], true);
        assert_eq!((other.ctx.format, other.app_name.as_str(), other.polish), (Markdown, "Chrome", false));
    }

    #[test]
    fn a_row_for_a_rule_covers_every_exe_the_rule_lists_unless_one_has_its_own() {
        let terminal =
            AppProfile { polish: Some(true), tone: Some("casual".into()), ..AppProfile::new("windowsterminal.exe") };
        let pwsh = AppProfile { tone: Some("formal".into()), ..AppProfile::new("pwsh.exe") };
        let rows = [terminal, pwsh];
        let r = exe("cmd.exe", "", &rows, false);
        assert_eq!((r.polish, r.tone.as_str(), r.ctx.code_mode), (true, "casual", true));
        let r = exe("pwsh.exe", "", &rows, false);
        assert_eq!((r.polish, r.tone.as_str()), (true, "formal"), "its own row first, field by field");
        assert!(!exe("code.exe", "", &rows, false).polish, "other rules' apps");
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
        let docs = resolve(&AppKey::exe("chrome.exe"), "Untitled - Google Docs", &rules, &[], defaults(false));
        assert_eq!((docs.ctx.format, docs.multiline_swap_safe, docs.app_name.as_str()), (Html, true, "chrome"));
        let other = resolve(&AppKey::exe("chrome.exe"), "Bolo", &rules, &[], defaults(false));
        assert_eq!((other.ctx.format, other.paste), (Markdown, Some(Paste::ShiftInsert)));
    }

    #[test]
    fn a_misspelt_field_is_an_error() {
        assert!(AppRules::parse("[[rule]]\nexe = [\"a.exe\"]\ncodemode = true").is_err());
        assert!(AppRules::parse("[[rule]]\nexe = [\"a.exe\"]\npaste = \"ctrl-shift-v\"").is_err());
        assert!(AppRules::parse("[[rule]]\npackages = [\"com.a\"]").is_err());
    }

    #[test]
    fn the_app_rows_say_where_each_value_comes_from() {
        let rules = AppRules::bundled();
        let profiles = [
            AppProfile { format: Some(Plain), ..AppProfile::new("slack.exe") },
            AppProfile { tone: Some("casual".into()), ..AppProfile::new("WhatsApp") },
            AppProfile::new("figma.exe"),
            AppProfile { polish: Some(false), ..AppProfile::for_app(&AppKey::package("com.example.notes")) },
        ];
        let rows = app_views(AppKind::Exe, &rules, &profiles, Defaults { polish: true, tone: "formal" });
        let bundled_apps = rules.rules.iter().filter(|r| r.title.is_none() && !r.exe.is_empty()).count();
        assert_eq!(rows.len(), bundled_apps + 2, "Gmail, a title rule, and Android apps have no row");

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
    fn the_android_app_rows_list_packages_only() {
        let rules = AppRules::bundled();
        let profiles = [
            AppProfile { polish: Some(false), ..AppProfile::new("slack.exe") },
            AppProfile { tone: Some("casual".into()), ..AppProfile::for_app(&AppKey::package("com.slack")) },
            AppProfile { polish: Some(false), ..AppProfile::for_app(&AppKey::package("com.example.notes")) },
        ];
        let rows = app_views(AppKind::Package, &rules, &profiles, defaults(true));
        let names: Vec<_> = rows.iter().map(|r| r.exe.as_str()).collect();
        assert_eq!(
            names,
            [
                "com.whatsapp",
                "com.google.android.gm",
                "com.android.chrome",
                "com.google.android.keep",
                "com.google.android.apps.messaging",
                "com.slack",
                "com.example.notes"
            ]
        );
        assert_eq!(rows[0].exes, ["com.whatsapp", "com.whatsapp.w4b"]);
        let slack = &rows[5];
        assert!(slack.bundled && slack.custom);
        assert_eq!((slack.fields.tone.value.as_str(), slack.fields.polish.value), ("casual", true), "not the exe row");
        let notes = &rows[6];
        assert!(!notes.bundled && notes.custom);
        assert_eq!((notes.fields.name.value.as_str(), notes.fields.polish.value), ("notes", false));
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
        let p = plan(&AppKey::exe("chrome.exe"), "Gmail", &rules, &prefs);
        assert_eq!(
            p.polish,
            Polish::On { tone: "neutral".into(), app: "Gmail".into(), dictionary: vec!["Bolo".into()] }
        );
        assert_eq!(p.resolved.ctx.format, Html);
        assert_eq!(plan(&AppKey::exe("code.exe"), "", &rules, &prefs).polish, Polish::Off);
        let Polish::On { app, .. } = plan(&AppKey::package("com.google.android.gm"), "", &rules, &prefs).polish else {
            panic!()
        };
        assert_eq!(app, "Gmail");
        prefs.apps = vec![AppProfile { tone: Some("casual".into()), ..AppProfile::new("slack.exe") }];
        let Polish::On { tone, app, .. } = plan(&AppKey::exe("slack.exe"), "", &rules, &prefs).polish else { panic!() };
        assert_eq!((tone.as_str(), app.as_str()), ("casual", "Slack"));
        prefs.has_key = false;
        assert_eq!(plan(&AppKey::exe("chrome.exe"), "Gmail", &rules, &prefs).polish, Polish::Off, "no key, no polish");
    }
}
