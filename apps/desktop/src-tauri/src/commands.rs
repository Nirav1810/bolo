//! The settings window's calls into the app (M3 ticket 09). Each change is saved to
//! `settings.toml` and applied to the running app at once; nothing needs a restart.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use bolo_core::OutputFormat;
use bolo_store::{History, PROVIDERS, ProviderUsage, Settings};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::desktop::Clipboard;
use crate::form::{CLOUD_ENGINES, Form};
use crate::keys::{self, Tested};
use crate::models::{self, DownloadEvent, ModelView};
use crate::{Bolo, key_name, lock, updates};

/// Download progress goes out on this event, as [`DownloadEvent`]s.
pub const DOWNLOAD_EVENT: &str = "bolo://download";

/// How many history rows the window lists.
const HISTORY_ROWS: usize = 200;

/// An app the bundled rules know, for the per-app polish list.
#[derive(Debug, Clone, Serialize)]
pub struct AppRow {
    pub name: String,
    /// The exe names; the first is the one an override is stored under.
    pub exes: Vec<String>,
    pub format: OutputFormat,
    pub code_mode: bool,
    /// The rule's polish choice; `None` follows the global switch.
    pub polish: Option<bool>,
}

/// A cloud engine the window offers.
#[derive(Debug, Clone, Serialize)]
pub struct CloudEngine {
    pub spec: &'static str,
    pub name: &'static str,
    pub provider: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct Paths {
    pub settings: String,
    pub models: String,
    pub history: String,
}

/// Everything the settings window shows.
#[derive(Debug, Clone, Serialize)]
pub struct SettingsView {
    pub form: Form,
    /// Provider → the stored key's hint (its last four characters), `None` for no key.
    pub keys: BTreeMap<&'static str, Option<String>>,
    /// Whether keys can be stored at all.
    pub keystore: bool,
    pub key_name: String,
    pub models: Vec<ModelView>,
    /// Models downloading now.
    pub downloading: Vec<String>,
    pub cloud: Vec<CloudEngine>,
    pub apps: Vec<AppRow>,
    pub version: &'static str,
    pub paths: Paths,
    pub ram_mb: u64,
    /// Onboarding's step while it's unfinished.
    pub onboarding_step: Option<u32>,
}

fn models_root(bolo: &Bolo) -> PathBuf {
    lock(&bolo.config).models_dir.clone()
}

fn view(bolo: &Bolo) -> SettingsView {
    let settings = lock(&bolo.settings).clone();
    let (keys, keystore) = {
        let secrets = lock(&bolo.secrets);
        let keys = PROVIDERS.iter().map(|&p| (p, secrets.as_ref().and_then(|s| s.hint(p)))).collect();
        (keys, secrets.is_some())
    };
    let ram_mb = models::total_ram_mb();
    let root = models_root(bolo);
    let models = bolo
        .manifest
        .as_ref()
        .map(|m| {
            let hinglish = settings.language != bolo_core::LanguageMode::English;
            let rec = models::recommend(m, ram_mb, hinglish).map(|e| e.id.clone());
            models::views(m, &root, rec.as_deref())
        })
        .unwrap_or_default();
    let apps = bolo
        .rules
        .rules
        .iter()
        .filter(|r| !r.exe.is_empty())
        .map(|r| AppRow {
            name: r.name.clone().unwrap_or_else(|| r.exe[0].trim_end_matches(".exe").to_owned()),
            exes: r.exe.clone(),
            format: r.format.unwrap_or_default(),
            code_mode: r.code_mode,
            polish: r.polish,
        })
        .collect();
    let path = |p: bolo_store::Result<PathBuf>| p.map(|p| p.display().to_string()).unwrap_or_default();
    SettingsView {
        form: Form::of(&settings),
        keys,
        keystore,
        key_name: key_name(settings.push_to_talk),
        models,
        downloading: bolo.downloads.running(),
        cloud: CLOUD_ENGINES.iter().map(|&(spec, name, provider)| CloudEngine { spec, name, provider }).collect(),
        apps,
        version: env!("CARGO_PKG_VERSION"),
        paths: Paths {
            settings: path(Settings::default_path()),
            models: root.display().to_string(),
            history: path(History::default_path()),
        },
        ram_mb,
        onboarding_step: settings.onboarding_step,
    }
}

#[tauri::command]
pub fn settings_get(bolo: State<'_, Bolo>) -> SettingsView {
    view(&bolo)
}

/// Save the window's form. The form as saved (tidied), and the push-to-talk key's name.
#[tauri::command]
pub fn settings_save(app: AppHandle, bolo: State<'_, Bolo>, form: Form) -> Result<SettingsView, String> {
    bolo.save_form(&app, form)?;
    Ok(view(&bolo))
}

/// Store `provider`'s key (blank deletes it). Its hint.
#[tauri::command]
pub fn key_save(bolo: State<'_, Bolo>, provider: String, key: String) -> Result<Option<String>, String> {
    bolo.save_key(&provider, &key)
}

/// Check `key`, or the stored key when it's `None`.
#[tauri::command]
pub async fn key_test(app: AppHandle, provider: String, key: Option<String>) -> Tested {
    let key = key.filter(|k| !k.trim().is_empty()).or_else(|| {
        let bolo = app.state::<Bolo>();
        let secrets = lock(&bolo.secrets);
        secrets.as_ref().and_then(|s| s.api_keys().get(&provider).map(str::to_owned))
    });
    match key {
        Some(key) => keys::test(&provider, &key).await,
        None => Tested::Failed { message: "no key to test".into() },
    }
}

/// While the window captures a new push-to-talk key, Bolo lets go of the old one, so
/// the press reaches the window (and Caps Lock doesn't start a dictation).
#[tauri::command]
pub fn ptt_capture(app: AppHandle, bolo: State<'_, Bolo>, capturing: bool) {
    if capturing {
        bolo.remove_hook();
    } else if !lock(&bolo.session).paused() {
        bolo.install_hook(&app);
    }
}

/// One history row, as the window lists it.
#[derive(Debug, Clone, Serialize)]
pub struct HistoryItem {
    pub id: i64,
    pub at_ms: i64,
    pub app: String,
    pub language: String,
    pub engine: String,
    pub final_text: String,
    pub polished_text: Option<String>,
    pub swap: Option<String>,
}

/// The newest rows whose text contains `query` (all rows for an empty query).
#[tauri::command]
pub fn history_search(bolo: State<'_, Bolo>, query: String) -> Result<Vec<HistoryItem>, String> {
    let rows = bolo.journal.with_history(|h| h.search(query.trim(), HISTORY_ROWS)).unwrap_or(Ok(Vec::new()));
    Ok(rows
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|r| HistoryItem {
            id: r.id,
            at_ms: r.dictation.at_ms,
            app: r.dictation.app,
            language: r.dictation.language,
            engine: r.dictation.engine,
            final_text: r.dictation.final_text,
            polished_text: r.dictation.polished_text,
            swap: r.dictation.swap,
        })
        .collect())
}

#[tauri::command]
pub fn history_delete(bolo: State<'_, Bolo>, id: i64) -> Result<(), String> {
    bolo.journal.with_history(|h| h.delete(id)).transpose().map(drop).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn history_clear(bolo: State<'_, Bolo>) -> Result<(), String> {
    bolo.journal.with_history(|h| h.clear()).transpose().map(drop).map_err(|e| e.to_string())
}

/// The usage meter's totals, as the Usage page shows them.
#[derive(Debug, Clone, Default, Serialize)]
pub struct UsageView {
    /// Since `today_start_ms`, the viewer's local midnight.
    pub today: Vec<ProviderUsage>,
    pub last_30_days: Vec<ProviderUsage>,
    pub all_time: Vec<ProviderUsage>,
    /// When the meter's oldest request was recorded.
    pub since_ms: Option<i64>,
}

const MS_PER_DAY: i64 = 86_400_000;

#[tauri::command]
pub fn usage_summary(bolo: State<'_, Bolo>, today_start_ms: i64) -> Result<UsageView, String> {
    let now = bolo_store::history::now_ms();
    bolo.journal
        .with_history(|h| -> bolo_store::Result<UsageView> {
            Ok(UsageView {
                today: h.usage_since(today_start_ms)?,
                last_30_days: h.usage_since(now - 30 * MS_PER_DAY)?,
                all_time: h.usage_since(0)?,
                since_ms: h.usage_started()?,
            })
        })
        .unwrap_or_else(|| Ok(UsageView::default()))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn usage_reset(bolo: State<'_, Bolo>) -> Result<(), String> {
    bolo.journal.with_history(|h| h.clear_usage()).transpose().map(drop).map_err(|e| e.to_string())
}

/// Put `text` on the clipboard (the History page's Copy).
#[tauri::command]
pub fn copy_text(bolo: State<'_, Bolo>, text: String) -> Result<(), String> {
    bolo.desktop
        .write_clipboard(&Clipboard { text: Some(text), ..Default::default() })
        .map(drop)
        .map_err(|e| e.to_string())
}

fn report(app: &AppHandle) -> models::Report {
    let app = app.clone();
    Arc::new(move |e: DownloadEvent| {
        if let Err(err) = app.emit(DOWNLOAD_EVENT, &e) {
            tracing::warn!(%err, "could not send download progress");
        }
    })
}

/// Start downloading model `id`; progress arrives on [`DOWNLOAD_EVENT`].
#[tauri::command]
pub fn model_download(app: AppHandle, bolo: State<'_, Bolo>, id: String) -> Result<(), String> {
    let manifest = bolo.manifest.as_ref().ok_or("the model list is unavailable")?;
    bolo.downloads.start(manifest, models_root(&bolo), &id, report(&app))
}

#[tauri::command]
pub fn model_cancel(app: AppHandle, bolo: State<'_, Bolo>, id: String) {
    bolo.downloads.cancel(&id, &report(&app));
}

/// Delete installed model `id`.
#[tauri::command]
pub fn model_remove(bolo: State<'_, Bolo>, id: String) -> Result<(), String> {
    let manifest = bolo.manifest.as_ref().ok_or("the model list is unavailable")?;
    let entry = manifest.get(&id).map_err(|e| e.to_string())?;
    let dir = entry.install_dir(&models_root(&bolo));
    if dir.exists() {
        // The engine may hold the model open: let it go first.
        if let Some(e) = lock(&bolo.engine).as_ref() {
            e.release_resources();
        }
        std::fs::remove_dir_all(&dir).map_err(|e| format!("could not delete {}: {e}", dir.display()))?;
    }
    Ok(())
}

/// Open one of Bolo's folders in Explorer: `settings`, `models` or `logs`.
#[tauri::command]
pub fn open_folder(bolo: State<'_, Bolo>, which: String) -> Result<(), String> {
    let dir = match which.as_str() {
        "settings" => bolo_store::config_dir().map_err(|e| e.to_string())?,
        "models" => models_root(&bolo),
        "logs" => models_root(&bolo).parent().map(|d| d.join("logs")).ok_or("no logs folder")?,
        other => return Err(format!("unknown folder {other}")),
    };
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    open(dir.as_os_str())
}

/// The links the windows may open outside the app.
const LINKS: [&str; 4] = [
    "https://github.com/Nirav1810/bolo",
    "https://github.com/Nirav1810/bolo/releases",
    "https://www.anthropic.com/legal/privacy",
    "ms-settings:sound",
];

/// Open `url` in the default browser (or Windows Settings). Only the [`LINKS`].
#[tauri::command]
pub fn open_link(url: String) -> Result<(), String> {
    if !LINKS.contains(&url.as_str()) {
        return Err(format!("not a link Bolo opens: {url}"));
    }
    open(std::ffi::OsStr::new(&url))
}

fn open(target: &std::ffi::OsStr) -> Result<(), String> {
    std::process::Command::new("explorer.exe").arg(target).spawn().map(drop).map_err(|e| e.to_string())
}

/// Mic levels during the check go out on this event, as an RMS number.
pub const MIC_EVENT: &str = "bolo://mic";

/// Onboarding's mic check: listen for three seconds on a cold open of the default mic.
#[tauri::command]
pub async fn mic_check(app: AppHandle) -> Result<crate::mic::MicCheck, String> {
    let levels = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::mic::check(|rms| {
            let _ = levels.emit(MIC_EVENT, rms);
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Onboarding reached `step` (0 welcome … 3 try it); `None` when it's finished.
#[tauri::command]
pub fn onboarding_save(app: AppHandle, bolo: State<'_, Bolo>, step: Option<u32>) -> Result<(), String> {
    bolo.change_settings(&app, |s| Settings { onboarding_step: step, ..s.clone() })
}

/// The About page's update section (ADR-0019).
#[tauri::command]
pub fn update_view(bolo: State<'_, Bolo>) -> updates::View {
    updates::View::now(bolo.updates.status())
}

/// Check now; the result arrives as an `update` event.
#[tauri::command]
pub fn update_check(app: AppHandle, bolo: State<'_, Bolo>) {
    bolo.check_for_update(&app, true);
}

/// Download and install the available update; Bolo restarts.
#[tauri::command]
pub fn update_install(app: AppHandle, bolo: State<'_, Bolo>) {
    bolo.install_update(&app);
}
