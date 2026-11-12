//! Bolo for Windows: the Tauri app that hosts one [`Engine`] in-process.
//!
//! - [`controller`] turns engine events into [`controller::AppEvent`]s for the webviews.
//! - [`session`] decides what a start, stop or pause does.
//! - [`desktop`] is the seam to the Windows desktop (window in front, clipboard, keys).
//! - [`rules`] maps the window in front to a target and a polish decision (app rules).
//! - [`insert`] pastes the text and swaps in polish (ADR-0018).
//! - [`journal`] records each dictation in history.
//! - [`pill`] is the overlay pill's state machine and [`overlay`] the thread and window
//!   that run it; [`pointer`] is the mouse over it (the idle button, hands-free).
//! - [`hotkey`] is the push-to-talk state machine and [`keyhook`] the hook that runs it.
//! - [`config`] lays `BOLO_*` variables over the engine config from settings.
//! - [`form`] is what the settings window edits, [`models`] the local models and their
//!   downloads, [`keys`] the API key check, [`mic`] onboarding's mic check; [`commands`]
//!   are the windows' calls.
//! - [`variant`] says whether this is Bolo or Bolo Nightly, and [`updates`] keeps it up
//!   to date from its own feed (ADR-0019).
//!
//! The app lives in the tray, runs as a single instance and keeps running when its
//! windows close. Holding the push-to-talk key (Caps Lock by default, ADR-0017) dictates;
//! the tray can start and stop one too. The text is pasted into the app in front.

mod billing;
mod commands;
pub mod config;
pub mod controller;
pub mod desktop;
pub mod form;
pub mod hotkey;
pub mod insert;
pub mod journal;
pub mod keyhook;
pub mod keys;
pub mod mic;
pub mod models;
pub mod overlay;
pub mod pill;
pub mod pointer;
pub mod rules;
pub mod session;
pub mod updates;
pub mod variant;

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use bolo_core::{ANTHROPIC, ApiKeys, BoloError, DictationState, Engine, EngineConfig, Polish};
use bolo_models::Manifest;
use bolo_store::{History, PushToTalkKey, Secrets, Settings};
use serde::Serialize;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, RunEvent, State, WindowEvent, Wry};
use tauri_plugin_autostart::ManagerExt;

use controller::{AppEvent, Controller, Emit};
use desktop::{Desktop, WinDesktop};
use form::{Changes, Form};
use hotkey::{Command, Flags};
use insert::{Inserter, ThreadTimer};
use journal::Journal;
use keyhook::{Handler, KeyHook};
use overlay::{Clicks, HitRegion, Overlay};
use rules::{AppRules, Plan, PolishPrefs};
use session::{Session, Toggled};
use updates::Updates;
use variant::Variant;

/// The one Tauri event every [`AppEvent`] is sent on.
pub const EVENT: &str = "bolo://event";

/// The settings window (settings, history, about, onboarding, debug), built when it's
/// opened and destroyed when it's closed ([`show_window`]).
const MAIN_WINDOW: &str = "main";
/// The overlay pill, created hidden at startup (`tauri.conf.json`).
const PILL_WINDOW: &str = "pill";

/// `BOLO_MEASURE=1` in a build with the `measure` feature: run beside an installed Bolo
/// for `scripts/idle_ram.ps1` without touching it. No single-instance check, no key or
/// mouse hook, no step-aside, no update checks; default settings in memory, no history,
/// no API keys and no log file, so nothing of the installed app's is read or written
/// except the models. Never on in shipped builds.
fn measuring() -> bool {
    static ON: LazyLock<bool> =
        LazyLock::new(|| cfg!(feature = "measure") && std::env::var_os("BOLO_MEASURE").is_some_and(|v| v == "1"));
    *ON
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The tray items whose labels follow the app's state.
struct TrayItems {
    toggle: MenuItem<Wry>,
    pause: MenuItem<Wry>,
}

/// Sends [`AppEvent`]s to every webview and keeps the tray labels in step.
struct TauriEmit {
    app: AppHandle,
    tray: TrayItems,
}

impl Emit for TauriEmit {
    fn emit(&self, event: AppEvent) {
        match &event {
            AppEvent::State { state } => {
                let label = if *state == DictationState::Listening { "Stop dictation" } else { "Start dictation" };
                let _ = self.tray.toggle.set_text(label);
            }
            AppEvent::Paused { paused } => {
                let _ = self.tray.pause.set_text(if *paused { "Resume Bolo" } else { "Pause Bolo" });
                let _ = self.tray.toggle.set_enabled(!paused);
            }
            _ => {}
        }
        if let Err(e) = self.app.emit(EVENT, event) {
            tracing::warn!(%e, "could not emit to the webviews");
        }
    }
}

/// The app's state, managed by Tauri.
struct Bolo {
    /// The settings as last saved; the settings window edits them.
    settings: Mutex<Settings>,
    /// The API keys in Credential Manager; `None` if it couldn't be opened.
    secrets: Mutex<Option<Secrets>>,
    config: Mutex<EngineConfig>,
    /// Built at startup; rebuilt on the next start if that failed (say, a model was
    /// missing and has since been downloaded).
    engine: Mutex<Option<Engine>>,
    session: Mutex<Session>,
    desktop: Arc<dyn Desktop>,
    /// The bundled app rules and the user's polish settings, for each dictation's plan.
    rules: AppRules,
    prefs: Mutex<PolishPrefs>,
    /// The last app seen in front that isn't Bolo, offered by the Apps tab's "Add app".
    recent_app: Mutex<Option<String>>,
    inserter: Arc<Inserter>,
    journal: Arc<Journal>,
    /// History's retention, read by the daily prune.
    retention_days: Arc<AtomicU32>,
    /// The bundled model manifest; `None` only if it's broken (a test catches that).
    manifest: Option<Manifest>,
    downloads: models::Downloads,
    /// Every event goes through the overlay, which drives the pill, to the webviews.
    overlay: Arc<Overlay>,
    /// Whether the idle button is on (the tray checkbox).
    idle_button: AtomicBool,
    emit: Arc<dyn Emit>,
    /// Why the config or the engine could not be built at startup, for the debug window.
    startup_error: Mutex<Option<String>>,
    /// Shared with the keyboard hook: whether a dictation is listening, and touches.
    flags: Arc<Flags>,
    push_to_talk: Mutex<PushToTalkKey>,
    /// The push-to-talk hook; `None` while paused or if it could not be installed.
    hook: Mutex<Option<KeyHook>>,
    /// The hook's thread id (0 for none), read without the `hook` lock by the inserter.
    hook_thread: Arc<AtomicU32>,
    /// The settings window let go of the hook to capture a new key ([`commands::ptt_capture`]).
    capturing: AtomicBool,
    /// This app is turned on in settings (`[variants]`), as last applied. Off, it stays
    /// paused: no hook, no pill, no model, until it is turned on again.
    on: AtomicBool,
    /// The self-updater's state (ADR-0019).
    updates: Arc<Updates>,
}

impl Bolo {
    fn engine(&self) -> Result<MutexGuard<'_, Option<Engine>>, BoloError> {
        let mut engine = lock(&self.engine);
        if engine.is_none() {
            let events = Box::new(Controller::new(
                self.emit.clone(),
                self.flags.clone(),
                self.inserter.clone(),
                self.journal.clone(),
            ));
            let config = lock(&self.config).clone();
            *engine = Some(Engine::new(config, events)?);
            lock(&self.session).engine_replaced();
            *lock(&self.startup_error) = None;
        }
        Ok(engine)
    }

    /// Load the model now, in the background, so a push-to-talk hold starts listening at
    /// once instead of waiting ~2.4 s on a load. It then stays loaded (the desktop's
    /// `idle_unload_secs` is 0). Not while paused or measuring, nor before onboarding is
    /// done, when the model may not be downloaded yet. Loading again is a no-op.
    fn warm(&self, settings: &Settings) {
        if measuring() || settings.onboarding_step.is_some() || lock(&self.session).paused() {
            return;
        }
        match self.engine() {
            Ok(engine) => {
                if let Some(e) = engine.as_ref() {
                    e.prewarm();
                }
            }
            Err(e) => tracing::debug!(%e, "no engine to warm"),
        }
    }

    /// The plan for a dictation into the window in front.
    fn plan(&self) -> Plan {
        let fg = self.desktop.foreground().unwrap_or_default();
        self.note_app(&fg.exe);
        rules::plan(&fg.exe, &fg.title, &self.rules, &lock(&self.prefs))
    }

    /// Remember `exe` as the last app in front, unless it's Bolo itself or the shell
    /// (the taskbar and the desktop are `explorer.exe`).
    fn note_app(&self, exe: &str) {
        static OWN: LazyLock<String> = LazyLock::new(|| {
            let exe = std::env::current_exe().ok();
            exe.and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase())).unwrap_or_default()
        });
        let exe = exe.trim().to_lowercase();
        if !exe.is_empty() && exe != *OWN && exe != "explorer.exe" {
            *lock(&self.recent_app) = Some(exe);
        }
    }

    /// A dictation started: tell the inserter how its text goes in, and history where.
    fn begin(&self, started: &Toggled) {
        if let Toggled::Started { app, target, polish } = started {
            self.inserter.begin(*target, *polish);
            self.journal.begin(app, target.format);
            self.overlay.begin(*polish);
        }
    }

    fn toggle(&self) {
        let result = self.engine().and_then(|engine| match engine.as_ref() {
            Some(e) => lock(&self.session).toggle(e, || self.plan()),
            None => Err(BoloError::Stopped),
        });
        match result {
            Ok(started @ Toggled::Started { .. }) => {
                self.begin(&started);
                tracing::info!(?started, "dictation started");
            }
            Ok(Toggled::Stopped) => tracing::info!("dictation stopped"),
            Ok(Toggled::Paused) => tracing::info!("paused; not starting"),
            Ok(Toggled::AlreadyListening) => {}
            Err(e) => {
                tracing::warn!(%e, "could not start or stop a dictation");
                self.emit.emit(AppEvent::Error { message: e.to_string() });
            }
        }
    }

    /// A click on the idle button: start a hands-free dictation, unless one is running.
    fn start_hands_free(&self) {
        if self.listening() {
            return;
        }
        self.toggle();
    }

    /// The hands-free Stop button: finish and insert.
    fn stop_hands_free(&self) {
        if self.listening() {
            self.inserter.released(Instant::now());
            self.toggle();
        }
    }

    /// The hands-free Cancel button.
    fn cancel(&self) {
        if let Some(e) = lock(&self.engine).as_ref() {
            e.cancel_dictation();
            self.inserter.drop_pending();
            tracing::info!("dictation cancelled from the pill");
        }
    }

    fn listening(&self) -> bool {
        lock(&self.engine).as_ref().is_some_and(|e| e.state() == DictationState::Listening)
    }

    /// What the push-to-talk key asks for (on the hook's command thread).
    fn push_to_talk(&self, command: Command) {
        let result = self.engine().and_then(|engine| {
            let Some(e) = engine.as_ref() else { return Err(BoloError::Stopped) };
            let mut session = lock(&self.session);
            match command {
                Command::Start => {
                    let t0 = Instant::now();
                    session.press(e, || self.plan()).map(|t| {
                        self.begin(&t);
                        // Hold recognised → listening: the mic open, plus a model load if
                        // the model wasn't warm.
                        let start_ms = t0.elapsed().as_millis() as u64;
                        tracing::info!(?t, start_ms, "push-to-talk held");
                    })
                }
                Command::Stop => {
                    // Key-up, for the key-up → paste latency the inserter logs.
                    self.inserter.released(Instant::now());
                    session.release(e).map(|stopped| tracing::info!(stopped, "push-to-talk released"))
                }
                Command::Cancel => {
                    e.cancel_dictation();
                    self.inserter.drop_pending();
                    tracing::info!("dictation cancelled with Esc");
                    Ok(())
                }
                Command::ReplayTap => Ok(()),
            }
        });
        if let Err(e) = result {
            tracing::warn!(%e, ?command, "push-to-talk failed");
            self.emit.emit(AppEvent::Error { message: e.to_string() });
        }
    }

    fn set_paused(&self, paused: bool, app: &AppHandle) {
        let engine = lock(&self.engine);
        lock(&self.session).set_paused(paused, engine.as_ref().map(|e| e as _));
        drop(engine);
        // Paused, Bolo lets go of the key entirely.
        if paused {
            self.remove_hook();
        } else {
            self.install_hook(app);
            let settings = lock(&self.settings).clone();
            self.warm(&settings);
        }
        tracing::info!(paused, "pause");
        self.emit.emit(AppEvent::Paused { paused });
    }

    fn install_hook(&self, app: &AppHandle) {
        let mut hook = lock(&self.hook);
        // Measuring beside an installed Bolo: never take its key or ask it to pause.
        if hook.is_some() || measuring() {
            return;
        }
        let key = *lock(&self.push_to_talk);
        // Bolo and Bolo Nightly never both hold the key: the other one pauses.
        let other = Variant::current().other();
        if variant::ask_to_step_aside(other) {
            tracing::info!(other = other.product_name(), "asked the other Bolo to pause");
        }
        match KeyHook::install(key, self.flags.clone(), PushToTalk(app.clone())) {
            Ok(h) => {
                tracing::info!(?key, "push-to-talk on");
                self.hook_thread.store(h.thread_id(), Ordering::Release);
                *hook = Some(h);
            }
            Err(e) => {
                tracing::error!(%e, "could not install the push-to-talk hook");
                self.emit.emit(AppEvent::Error { message: format!("push-to-talk is unavailable: {e}") });
            }
        }
    }

    /// Unhook. The hook is dropped (and its threads joined) outside the lock: its command
    /// thread may be waiting on it.
    fn remove_hook(&self) {
        self.hook_thread.store(0, Ordering::Release);
        let hook = lock(&self.hook).take();
        drop(hook);
    }

    /// Let go of the keyboard, close the mic and unload the model: before Quit, and before
    /// an update's installer replaces the exe.
    fn shut_down(&self) {
        self.remove_hook();
        drop(lock(&self.engine).take());
    }

    /// This app's on/off switch is now `on`: pause it for good, or resume it. Nothing
    /// happens unless the switch itself changed, so a pause from the tray stays.
    fn apply_switch(&self, app: &AppHandle, on: bool) {
        if self.on.swap(on, Ordering::AcqRel) == on {
            return;
        }
        tracing::info!(on, "turned {} in settings", if on { "on" } else { "off" });
        let paused = lock(&self.session).paused();
        if on == paused {
            self.set_paused(!on, app);
        }
    }

    /// The other app changed the on/off switches: read them from the file again.
    fn reload_switches(&self, app: &AppHandle) {
        let variants = match Settings::default_path().and_then(|p| Settings::load(&p)) {
            Ok(loaded) => loaded.settings.variants,
            Err(e) => {
                tracing::warn!(%e, "could not read the switches the other Bolo changed");
                return;
            }
        };
        let on = Variant::current().is_on(&variants);
        lock(&self.settings).variants = variants;
        self.apply_switch(app, on);
    }

    /// The other Bolo started or resumed: pause, and say why.
    fn step_aside(&self, app: &AppHandle) {
        if lock(&self.session).paused() {
            return;
        }
        let other = Variant::current().other().product_name();
        tracing::info!(other, "pausing: the other Bolo took push-to-talk");
        self.set_paused(true, app);
        self.emit.emit(AppEvent::Notice {
            message: format!("Paused while {other} runs"),
            hint: Some("Resume from the tray".into()),
        });
    }

    /// Look for an update on a thread of its own.
    fn check_for_update(&self, app: &AppHandle, manual: bool) {
        let app = app.clone();
        let spawned = std::thread::Builder::new().name("bolo-update-check".into()).spawn(move || {
            let exit_app = app.clone();
            let before_exit: updates::BeforeExit = Arc::new(move || exit_app.state::<Bolo>().shut_down());
            updates::check(&app, &app.state::<Bolo>().updates, manual, before_exit);
        });
        if let Err(e) = spawned {
            tracing::warn!(%e, "could not start an update check");
        }
    }

    /// Install the update the last check found, on a thread of its own.
    fn install_update(&self, app: &AppHandle) {
        let app = app.clone();
        let spawned = std::thread::Builder::new()
            .name("bolo-update-install".into())
            .spawn(move || updates::install(&app, &app.state::<Bolo>().updates));
        if let Err(e) = spawned {
            tracing::warn!(%e, "could not start the update");
        }
    }

    fn toggle_paused(&self, app: &AppHandle) {
        // Turned off in settings: Resume turns it back on, so it stays on after a restart.
        if !self.on.load(Ordering::Acquire) {
            let turned_on = self.change_settings(app, |s| {
                let mut s = s.clone();
                match Variant::current() {
                    Variant::Release => s.variants.release = true,
                    Variant::Nightly => s.variants.nightly = true,
                }
                s
            });
            if let Err(e) = turned_on {
                tracing::warn!(%e, "could not turn Bolo on");
            }
            return;
        }
        let paused = lock(&self.session).paused();
        self.set_paused(!paused, app);
    }

    /// Change the settings, save them and make the app follow.
    fn change_settings(&self, app: &AppHandle, change: impl FnOnce(&Settings) -> Settings) -> Result<(), String> {
        let mut current = lock(&self.settings);
        let new = change(&current);
        if new == *current {
            return Ok(());
        }
        let changes = form::changes(&current, &new);
        if !measuring() {
            Settings::default_path().and_then(|path| new.save(&path)).map_err(|e| e.to_string())?;
        }
        *current = new.clone();
        drop(current);
        self.follow(app, &new, changes);
        Ok(())
    }

    /// The window's form, saved and applied. The form as saved (tidied).
    fn save_form(&self, app: &AppHandle, form: Form) -> Result<Form, String> {
        self.change_settings(app, |s| form.apply(s))?;
        Ok(Form::of(&lock(&self.settings)))
    }

    /// Make the running app match `settings` after `changes`.
    fn follow(&self, app: &AppHandle, settings: &Settings, changes: Changes) {
        if changes.engine || changes.polish || changes.snippets || changes.mic {
            self.reconfigure(settings, changes.engine);
        }
        // A new engine config carries the snippets already; otherwise send them alone,
        // so the model stays loaded.
        if changes.snippets && !changes.engine {
            let snippets = lock(&self.config).snippets.clone();
            if let Some(e) = lock(&self.engine).as_ref() {
                e.set_snippets(snippets);
            }
        }
        // The same for the microphone: a warm mic moves to the new device.
        if changes.mic && !changes.engine {
            let device = lock(&self.config).input_device.clone();
            tracing::info!(?device, "microphone changed");
            if let Some(e) = lock(&self.engine).as_ref() {
                e.set_input_device(device);
            }
        }
        if changes.key {
            *lock(&self.push_to_talk) = settings.push_to_talk;
            self.overlay.set_key(key_name(settings.push_to_talk));
            if lock(&self.hook).is_some() {
                self.remove_hook();
                self.install_hook(app);
            }
        }
        if changes.autostart {
            let autolaunch = app.autolaunch();
            let done = if settings.autostart { autolaunch.enable() } else { autolaunch.disable() };
            if let Err(e) = done {
                tracing::warn!(%e, on = settings.autostart, "could not change start with Windows");
                self.emit.emit(AppEvent::Error { message: format!("Start with Windows: {e}") });
            }
        }
        if changes.history {
            self.journal.with_history(|h| h.set_enabled(settings.history.enabled));
        }
        if changes.retention {
            self.retention_days.store(settings.history.retention_days, Ordering::Relaxed);
            self.journal.with_history(|h| prune(h, settings.history.retention_days));
        }
        if changes.variants {
            self.apply_switch(app, Variant::current().is_on(&settings.variants));
            variant::ask_to_reload(Variant::current().other());
        }
        if changes.warm {
            self.warm(settings);
        }
    }

    /// Rebuild the engine config from `settings` and the stored keys. With `engine`, the
    /// engine gets it too (its model reloads on the next dictation); otherwise only the
    /// polish plan changes.
    fn reconfigure(&self, settings: &Settings, engine: bool) {
        let keys = lock(&self.secrets).as_ref().map(|s| s.api_keys().clone()).unwrap_or_default();
        let (config, error) = engine_config(settings, keys);
        *lock(&self.prefs) = polish_prefs(settings, &config);
        if engine {
            tracing::info!(engines = ?config.engines, language = ?config.language, "engine config changed");
            if let Some(e) = lock(&self.engine).as_ref() {
                e.update_config(config.clone());
            }
            // The new config carries its own polish; send the plan's again.
            lock(&self.session).engine_replaced();
            *lock(&self.startup_error) = error;
        }
        *lock(&self.config) = config;
    }

    /// Store (or, blank, delete) `provider`'s key. Its hint for the window.
    fn save_key(&self, provider: &str, key: &str) -> Result<Option<String>, String> {
        let hint = {
            let mut secrets = lock(&self.secrets);
            let s = secrets.as_mut().ok_or("Windows Credential Manager is unavailable")?;
            s.set(provider, key).map_err(|e| e.to_string())?;
            s.hint(provider)
        };
        let settings = lock(&self.settings).clone();
        self.reconfigure(&settings, true);
        self.warm(&settings);
        Ok(hint)
    }
}

/// Hands the hook's commands to the app's [`Bolo`] state.
struct PushToTalk(AppHandle);

impl Handler for PushToTalk {
    fn command(&self, command: Command) {
        self.0.state::<Bolo>().push_to_talk(command);
    }
}

#[derive(Serialize)]
struct Status {
    state: DictationState,
    paused: bool,
    error: Option<String>,
}

#[tauri::command]
fn status(bolo: State<'_, Bolo>) -> Status {
    let state = lock(&bolo.engine).as_ref().map_or(DictationState::Idle, Engine::state);
    Status { state, paused: lock(&bolo.session).paused(), error: lock(&bolo.startup_error).clone() }
}

#[tauri::command]
fn toggle_dictation(bolo: State<'_, Bolo>) {
    bolo.toggle();
}

/// What the pill shows now, for the pill webview as it loads.
#[tauri::command]
fn pill_view(bolo: State<'_, Bolo>) -> pill::View {
    bolo.overlay.view()
}

/// The pill webview reports where its clickable parts are.
#[tauri::command]
fn pill_regions(bolo: State<'_, Bolo>, regions: Vec<HitRegion>) {
    bolo.overlay.regions(regions);
}

/// What the pill's buttons do, run off the overlay's thread (a start can load the model).
struct PillClicks(AppHandle);

impl PillClicks {
    fn later(&self, name: &str, f: impl FnOnce(&Bolo) + Send + 'static) {
        let app = self.0.clone();
        let spawned =
            std::thread::Builder::new().name(format!("bolo-pill-{name}")).spawn(move || f(&app.state::<Bolo>()));
        if let Err(e) = spawned {
            tracing::warn!(%e, "could not act on a pill click");
        }
    }
}

impl Clicks for PillClicks {
    fn start(&self) {
        self.later("start", Bolo::start_hands_free);
    }
    fn stop(&self) {
        self.later("stop", Bolo::stop_hands_free);
    }
    fn cancel(&self) {
        self.later("cancel", Bolo::cancel);
    }
    fn save_position(&self, position: f64) {
        let bolo = self.0.state::<Bolo>();
        let saved = bolo.change_settings(&self.0, |s| {
            let mut s = s.clone();
            s.overlay.position = position;
            s
        });
        if let Err(e) = saved {
            tracing::warn!(%e, "could not save the pill's position");
        }
    }
}

/// The push-to-talk key's name, as the idle hint and the settings window show it.
pub(crate) fn key_name(key: PushToTalkKey) -> String {
    match key {
        PushToTalkKey::CapsLock => "Caps Lock".into(),
        PushToTalkKey::RightCtrl => "Right Ctrl".into(),
        PushToTalkKey::RightAlt => "Right Alt".into(),
        other => format!("{other:?}"),
    }
}

/// Show the main window on a page: `debug`, `settings`, `onboarding` or a settings tab
/// (`history`, `about`, ...). Closing the window destroys its webview, which gives back
/// the WebView2 memory it held while hidden (M4 ticket 04), so this builds it again when
/// it's gone. Called on the main thread (the tray, setup, a second launch).
fn show_window(app: &AppHandle, page: &str) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        let _ = window.eval(format!("location.hash = '#{page}'"));
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    let url = tauri::WebviewUrl::App(format!("index.html#{page}").into());
    // The page sets its own title once it loads (App.svelte).
    let built = tauri::WebviewWindowBuilder::new(app, MAIN_WINDOW, url)
        .title(format!("{} Settings", Variant::current().product_name()))
        .inner_size(980.0, 700.0)
        .min_inner_size(760.0, 520.0)
        .center()
        .focused(true)
        .build();
    if let Err(e) = built {
        tracing::warn!(%e, page, "could not open the window");
    }
}

/// The main window closed and its webview is gone. If it was capturing a new
/// push-to-talk key, the page had no chance to hand the key back: take it back here.
/// (Only then: on Quit the hook is already gone on purpose.)
fn main_window_destroyed(app: &AppHandle) {
    let bolo = app.state::<Bolo>();
    if bolo.capturing.swap(false, Ordering::AcqRel) && !lock(&bolo.session).paused() {
        bolo.install_hook(app);
    }
}

fn init_logging() {
    use tracing_subscriber::EnvFilter;
    use tracing_subscriber::fmt::writer::MakeWriterExt;

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    // %LOCALAPPDATA%\Bolo\data\logs, next to the models (ADR-007).
    let file = bolo_models::default_models_root()
        .filter(|_| !measuring())
        .and_then(|m| m.parent().map(|d| d.join("logs")))
        .and_then(|dir| std::fs::create_dir_all(&dir).ok().map(|()| dir.join(Variant::current().log_file())))
        .and_then(|path| std::fs::OpenOptions::new().create(true).append(true).open(path).ok());
    let builder = tracing_subscriber::fmt().with_env_filter(filter).with_ansi(false);
    let _ = match file {
        Some(f) => builder.with_writer(std::io::stderr.and(Mutex::new(f))).try_init(),
        None => builder.try_init(),
    };
}

fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let toggle = MenuItem::with_id(app, "toggle", "Start dictation", true, None::<&str>)?;
    let pause = MenuItem::with_id(app, "pause", "Pause Bolo", true, None::<&str>)?;
    let (mut settings, fresh) = if measuring() {
        // The defaults, minus the on-screen button: no second pill over the installed one's.
        let mut s = Settings::default();
        s.overlay.idle_button = false;
        (s, false)
    } else {
        load_settings()
    };
    if fresh {
        settings.onboarding_step = Some(0);
        if let Err(e) = Settings::default_path().and_then(|p| settings.save(&p)) {
            tracing::warn!(%e, "could not save the settings");
        }
    }
    let onboarding = settings.onboarding_step.is_some();
    let idle_button = CheckMenuItem::with_id(
        app,
        "idle-button",
        "Show button on screen",
        true,
        settings.overlay.idle_button,
        None::<&str>,
    )?;
    // Which build this is, first in the menu: a Nightly must never pass for a release.
    let header = MenuItem::with_id(app, "version", variant::title(variant::VERSION), false, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &header,
            &PredefinedMenuItem::separator(app)?,
            &toggle,
            &pause,
            &idle_button,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?,
            &MenuItem::with_id(app, "history", "History…", true, None::<&str>)?,
            &MenuItem::with_id(app, "onboarding", "Set up Bolo…", true, None::<&str>)?,
            &MenuItem::with_id(app, "debug", "Debug window", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "quit", "Quit Bolo", true, None::<&str>)?,
        ],
    )?;
    // Shown at the top of the menu only while an update is available.
    let update_item = MenuItem::with_id(app, "update", "Update Bolo", true, None::<&str>)?;
    let mut tray = TrayIconBuilder::with_id("bolo")
        .tooltip(variant::title(variant::VERSION))
        .menu(&menu)
        .show_menu_on_left_click(true);
    if let Some(icon) = app.default_window_icon() {
        // Bolo Nightly builds with its own (violet) icons: tauri.nightly.conf.json.
        tray = tray.icon(icon.clone());
    }
    tray.on_menu_event(|app, event| {
        let bolo = app.state::<Bolo>();
        match event.id.as_ref() {
            "toggle" => bolo.toggle(),
            "pause" => bolo.toggle_paused(app),
            "idle-button" => {
                let on = !bolo.idle_button.fetch_xor(true, Ordering::Relaxed);
                bolo.overlay.set_idle_button(on);
                let saved = bolo.change_settings(app, |s| {
                    let mut s = s.clone();
                    s.overlay.idle_button = on;
                    s
                });
                if let Err(e) = saved {
                    tracing::warn!(%e, "could not save the idle button setting");
                }
            }
            "settings" => show_window(app, "settings"),
            "history" => show_window(app, "history"),
            "onboarding" => show_window(app, "onboarding"),
            "debug" => show_window(app, "debug"),
            "update" => bolo.install_update(app),
            "quit" => {
                bolo.shut_down();
                app.exit(0);
            }
            _ => {}
        }
    })
    .build(app)?;

    let webviews: Arc<dyn Emit> = Arc::new(TauriEmit { app: app.handle().clone(), tray: TrayItems { toggle, pause } });
    let overlay = Overlay::spawn(
        webviews,
        pill_window(app),
        pointer_hook(),
        Arc::new(PillClicks(app.handle().clone())),
        overlay::Config {
            idle_button: settings.overlay.idle_button,
            position: settings.overlay.position,
            key: key_name(settings.push_to_talk),
        },
    );
    let emit: Arc<dyn Emit> = overlay.clone();
    let updates = Arc::new(Updates::new(emit.clone(), move |version| {
        let shown = menu.get("update").is_some();
        match version {
            Some(v) => {
                let _ = update_item.set_text(format!("Restart to update to {v}"));
                // Under the version line and its separator.
                if !shown && let Err(e) = menu.insert(&update_item, 2) {
                    tracing::warn!(%e, "could not add the update item to the tray");
                }
            }
            None if shown => {
                let _ = menu.remove(&update_item);
            }
            None => {}
        }
    }));
    let secrets = if measuring() {
        None
    } else {
        Secrets::os().inspect_err(|e| tracing::warn!(%e, "could not open the credential store; no API keys")).ok()
    };
    let keys = secrets.as_ref().map(|s| s.api_keys().clone()).unwrap_or_default();
    let (config, config_error) = engine_config(&settings, keys);
    tracing::info!(engines = ?config.engines, language = ?config.language, models = %config.models_dir.display(), "engine config");
    let prefs = polish_prefs(&settings, &config);
    let journal = Arc::new(Journal::new(if measuring() { None } else { open_history(&settings) }));
    let retention_days = Arc::new(AtomicU32::new(settings.history.retention_days));
    keep_pruning(journal.clone(), retention_days.clone());
    let push_to_talk: PushToTalkKey = settings.push_to_talk;
    let manifest =
        Manifest::bundled().inspect_err(|e| tracing::error!(%e, "the bundled model manifest is broken")).ok();
    let desktop: Arc<dyn Desktop> = Arc::new(WinDesktop);
    let flags = Arc::new(Flags::default());
    let hook_thread = Arc::new(AtomicU32::new(0));
    let thread = hook_thread.clone();
    // The pending swap's mouse hook comes and goes with `Flags::watching`.
    let inserter = Inserter::new(desktop.clone(), Arc::new(ThreadTimer), flags.clone(), move || {
        keyhook::watch_changed(thread.load(Ordering::Acquire));
    });
    let bolo = Bolo {
        settings: Mutex::new(settings.clone()),
        secrets: Mutex::new(secrets),
        config: Mutex::new(config),
        engine: Mutex::new(None),
        session: Mutex::new(Session::default()),
        desktop,
        rules: AppRules::bundled(),
        prefs: Mutex::new(prefs),
        recent_app: Mutex::new(None),
        inserter,
        journal,
        retention_days,
        manifest,
        downloads: models::Downloads::default(),
        overlay,
        idle_button: AtomicBool::new(settings.overlay.idle_button),
        emit,
        startup_error: Mutex::new(config_error),
        flags,
        push_to_talk: Mutex::new(push_to_talk),
        hook: Mutex::new(None),
        hook_thread,
        capturing: AtomicBool::new(false),
        on: AtomicBool::new(Variant::current().is_on(&settings.variants)),
        updates,
    };
    if let Err(e) = bolo.engine() {
        tracing::error!(%e, "the engine could not start; it will be retried on the next dictation");
        lock(&bolo.startup_error).get_or_insert(e.to_string());
    }
    app.manage(bolo);
    if measuring() {
        tracing::warn!("BOLO_MEASURE: no hooks, no update checks, nothing saved");
        // `BOLO_MEASURE_OPEN=settings` opens that page, to measure it open and then closed.
        if let Ok(page) = std::env::var("BOLO_MEASURE_OPEN") {
            show_window(app.handle(), &page);
        }
        return Ok(());
    }
    let bolo = app.state::<Bolo>();
    if bolo.on.load(Ordering::Acquire) {
        bolo.install_hook(app.handle());
        // After the hook, which may ask the other Bolo to pause (and so free its model).
        let settings = lock(&bolo.settings).clone();
        bolo.warm(&settings);
    } else {
        // Turned off in settings: stay paused, and leave the key to the other Bolo.
        tracing::info!("turned off in settings; staying paused");
        bolo.set_paused(true, app.handle());
    }
    keep_checking_for_updates(app.handle().clone());
    let handle = app.handle().clone();
    if let Err(e) = variant::listen_for_step_aside(Variant::current(), move || {
        handle.state::<Bolo>().step_aside(&handle);
    }) {
        tracing::warn!(%e, "could not listen for the other Bolo");
    }
    let handle = app.handle().clone();
    if let Err(e) = variant::listen_for_reload(Variant::current(), move || {
        handle.state::<Bolo>().reload_switches(&handle);
    }) {
        tracing::warn!(%e, "could not listen for the other Bolo's switches");
    }
    // First run (or onboarding left unfinished): open it.
    if onboarding {
        show_window(app.handle(), "onboarding");
    } else if let Some(from) = updates::marker_path().and_then(|m| updates::take_updated(&m, variant::VERSION)) {
        // The restart after an update: the installer starts Bolo hidden in the tray, which
        // looks as if it never came back. Show About, which says what changed.
        tracing::info!(%from, "updated");
        app.state::<Bolo>().updates.set(updates::Status::Updated { from });
        show_window(app.handle(), "about");
    }
    Ok(())
}

/// The pill window, made click-through and non-activating.
fn pill_window(app: &tauri::App) -> Box<dyn overlay::PillWindow> {
    /// Stands in when the window is missing or can't be set up: no pill.
    struct NoPill;
    impl overlay::PillWindow for NoPill {
        fn monitor(&self) -> Option<overlay::Monitor> {
            None
        }
        fn show_at(&self, _: pill::Rect) {}
        fn move_to(&self, _: pill::Rect) {}
        fn hide(&self) {}
    }
    let Some(window) = app.get_webview_window(PILL_WINDOW) else {
        tracing::warn!("no pill window");
        return Box::new(NoPill);
    };
    #[cfg(windows)]
    match overlay::win::WinPill::new(app.handle().clone(), &window) {
        Ok(pill) => return Box::new(pill),
        Err(e) => tracing::warn!(%e, "could not set up the pill window"),
    }
    let _ = window;
    Box::new(NoPill)
}

/// The mouse hook for the pill's clicks, hover and drag; `None` if it can't start (the
/// pill then only shows).
fn pointer_hook() -> Option<(Box<dyn overlay::Pointer>, std::sync::mpsc::Receiver<pointer::Action>)> {
    if measuring() {
        return None;
    }
    #[cfg(windows)]
    match pointer::win::PointerHook::start() {
        Ok((hook, actions)) => return Some((Box::new(hook), actions)),
        Err(e) => tracing::warn!(%e, "could not start the pill's mouse hook"),
    }
    None
}

/// The settings file, or the defaults if it can't be read; and whether there was no
/// file (Bolo's first run here).
fn load_settings() -> (Settings, bool) {
    match Settings::default_path().and_then(|p| Settings::load(&p)) {
        Ok(loaded) => {
            if let Some(backup) = &loaded.recovered_from {
                tracing::warn!(backup = %backup.display(), "the settings file was bad; it was replaced with the defaults");
            }
            (loaded.settings, loaded.fresh)
        }
        Err(e) => {
            tracing::warn!(%e, "could not read the settings; using the defaults");
            (Settings::default(), false)
        }
    }
}

/// The engine config from settings and the stored keys, with any `BOLO_*` variables on
/// top. A bad variable is reported and ignored.
fn engine_config(settings: &Settings, keys: ApiKeys) -> (EngineConfig, Option<String>) {
    let base = settings.engine_config_with_keys(keys);
    match config::engine_config(|k| std::env::var(k).ok(), base.clone()) {
        Ok(c) => (c, None),
        Err(e) => {
            tracing::error!(%e, "bad BOLO_* variable; using the settings");
            (base, Some(e))
        }
    }
}

/// The polish choices each dictation is planned with.
fn polish_prefs(settings: &Settings, config: &EngineConfig) -> PolishPrefs {
    PolishPrefs {
        // Without the key polish never runs, so the switch only matters with one; then
        // the config says whether settings (or BOLO_POLISH) turned it on.
        enabled: matches!(config.polish, Polish::On { .. }),
        tone: match &config.polish {
            Polish::On { tone, .. } => tone.clone(),
            Polish::Off => settings.polish.tone.clone(),
        },
        dictionary: config.dictionary.clone(),
        apps: settings.apps.clone(),
        has_key: config.api_keys.get(ANTHROPIC).is_some(),
    }
}

/// The history database, recording or not as settings say, pruned now. `None` if it
/// can't be opened: dictation still works, unrecorded.
fn open_history(settings: &Settings) -> Option<History> {
    let opened = History::default_path().and_then(|p| History::open(&p));
    let mut history = opened.inspect_err(|e| tracing::warn!(%e, "could not open history")).ok()?;
    history.set_enabled(settings.history.enabled);
    prune(&mut history, settings.history.retention_days);
    Some(history)
}

fn prune(history: &mut History, retention_days: u32) {
    match history.prune(retention_days, bolo_store::history::now_ms()) {
        Ok(0) => {}
        Ok(n) => tracing::info!(n, retention_days, "pruned old history"),
        Err(e) => tracing::warn!(%e, "could not prune history"),
    }
}

/// Prune history once a day while Bolo runs, with the retention of the day.
fn keep_pruning(journal: Arc<Journal>, retention_days: Arc<AtomicU32>) {
    const DAY: Duration = Duration::from_secs(24 * 60 * 60);
    let spawned = std::thread::Builder::new().name("bolo-history-prune".into()).spawn(move || {
        loop {
            std::thread::sleep(DAY);
            journal.with_history(|h| prune(h, retention_days.load(Ordering::Relaxed)));
        }
    });
    if let Err(e) = spawned {
        tracing::warn!(%e, "could not start the daily history prune");
    }
}

/// Check for an update shortly after launch, then once a day, while `[updates]
/// auto_check` is on (ADR-0019). The switch is read at every tick, so it takes effect
/// without a restart.
fn keep_checking_for_updates(app: AppHandle) {
    let spawned = std::thread::Builder::new().name("bolo-update-schedule".into()).spawn(move || {
        std::thread::sleep(updates::FIRST_CHECK_AFTER);
        let mut last: Option<Instant> = None;
        loop {
            let bolo = app.state::<Bolo>();
            let on = lock(&bolo.settings).updates.auto_check;
            if updates::check_due(on, last.map(|at| at.elapsed())) {
                last = Some(Instant::now());
                bolo.check_for_update(&app, false);
            }
            std::thread::sleep(updates::SCHEDULE_TICK);
        }
    });
    if let Err(e) = spawned {
        tracing::warn!(%e, "could not schedule update checks");
    }
}

/// Run the app until the user quits from the tray.
pub fn run() {
    init_logging();
    tracing::info!(version = variant::VERSION, app = Variant::current().product_name(), "Bolo desktop starting");
    let mut builder = tauri::Builder::default();
    if !measuring() {
        // First, so a second launch exits before it builds anything.
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_window(app, "settings")));
    }
    let app = builder
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            status,
            toggle_dictation,
            pill_view,
            pill_regions,
            commands::settings_get,
            commands::settings_save,
            commands::app_set,
            commands::app_reset,
            commands::app_add,
            commands::key_save,
            commands::key_test,
            commands::ptt_capture,
            commands::history_search,
            commands::history_delete,
            commands::history_clear,
            commands::usage_summary,
            commands::usage_reset,
            commands::usage_reports,
            commands::copy_text,
            commands::model_download,
            commands::model_cancel,
            commands::model_remove,
            commands::open_folder,
            commands::open_link,
            commands::mic_check,
            commands::microphones,
            commands::onboarding_save,
            commands::update_view,
            commands::update_check,
            commands::update_install,
        ])
        .setup(setup)
        .on_window_event(|window, event| match event {
            // Closing the main window (settings, history, onboarding) destroys it, so its
            // webview's memory goes back (M4 ticket 04); `show_window` builds it again.
            // Bolo keeps running in the tray. Any other window (the pill) only hides.
            WindowEvent::CloseRequested { api, .. } if window.label() != MAIN_WINDOW => {
                api.prevent_close();
                let _ = window.hide();
            }
            WindowEvent::Destroyed if window.label() == MAIN_WINDOW => main_window_destroyed(window.app_handle()),
            // The user went from Settings to another app: note it, so the Apps tab can
            // offer it on the way back. A moment later, once that app is in front.
            WindowEvent::Focused(false) if window.label() == MAIN_WINDOW => {
                let app = window.app_handle().clone();
                let _ = std::thread::Builder::new().name("bolo-recent-app".into()).spawn(move || {
                    std::thread::sleep(Duration::from_millis(400));
                    let bolo = app.state::<Bolo>();
                    if let Some(fg) = bolo.desktop.foreground() {
                        bolo.note_app(&fg.exe);
                    }
                });
            }
            _ => {}
        })
        .build(tauri::generate_context!());
    let app = match app {
        Ok(app) => app,
        Err(e) => {
            tracing::error!(%e, "Bolo could not start");
            std::process::exit(1);
        }
    };
    app.run(|_, event| {
        // With every window hidden there is nothing open, and that must not quit Bolo.
        if let RunEvent::ExitRequested { code: None, api, .. } = event {
            api.prevent_exit();
        }
    });
}
