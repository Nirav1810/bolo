//! Bolo for Windows: the Tauri app that hosts one [`Engine`] in-process.
//!
//! - [`controller`] turns engine events into [`controller::AppEvent`]s for the webviews.
//! - [`session`] decides what a start, stop or pause does.
//! - [`desktop`] is the seam to the Windows desktop (window in front, clipboard, keys).
//! - [`insert`] pastes the text and swaps in polish (ADR-0018).
//! - [`hotkey`] is the push-to-talk state machine and [`keyhook`] the hook that runs it.
//! - [`config`] builds the engine config from `BOLO_*` variables until `bolo-store`.
//!
//! The app lives in the tray, runs as a single instance and keeps running when its
//! windows close. Holding the push-to-talk key (Caps Lock by default, ADR-0017) dictates;
//! the tray can start and stop one too. The text is pasted into the app in front.

pub mod config;
pub mod controller;
pub mod desktop;
pub mod hotkey;
pub mod insert;
pub mod keyhook;
pub mod session;

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Instant;

use bolo_core::{BoloError, DictationState, Engine, EngineConfig, Polish};
use bolo_store::{PushToTalkKey, Settings};
use serde::Serialize;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, RunEvent, State, WindowEvent, Wry};

use controller::{AppEvent, Controller, Emit};
use desktop::{Desktop, WinDesktop};
use hotkey::{Command, Flags};
use insert::{Inserter, Target, ThreadTimer};
use keyhook::{Handler, KeyHook};
use session::{Session, Toggled};

/// The one Tauri event every [`AppEvent`] is sent on.
pub const EVENT: &str = "bolo://event";

const MAIN_WINDOW: &str = "main";

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
    config: EngineConfig,
    /// Built at startup; rebuilt on the next start if that failed (say, a model was
    /// missing and has since been downloaded).
    engine: Mutex<Option<Engine>>,
    session: Mutex<Session>,
    desktop: Arc<dyn Desktop>,
    inserter: Arc<Inserter>,
    emit: Arc<dyn Emit>,
    /// Why the config or the engine could not be built at startup, for the debug window.
    startup_error: Mutex<Option<String>>,
    /// Shared with the keyboard hook: whether a dictation is listening, and touches.
    flags: Arc<Flags>,
    push_to_talk: PushToTalkKey,
    /// The push-to-talk hook; `None` while paused or if it could not be installed.
    hook: Mutex<Option<KeyHook>>,
    /// The hook's thread id (0 for none), read without the `hook` lock by the inserter.
    hook_thread: Arc<AtomicU32>,
}

impl Bolo {
    fn engine(&self) -> Result<MutexGuard<'_, Option<Engine>>, BoloError> {
        let mut engine = lock(&self.engine);
        if engine.is_none() {
            let events = Box::new(Controller::new(self.emit.clone(), self.flags.clone(), self.inserter.clone()));
            *engine = Some(Engine::new(self.config.clone(), events)?);
            *lock(&self.startup_error) = None;
        }
        Ok(engine)
    }

    /// A dictation started: tell the inserter how its text goes in. Every app gets the
    /// defaults until the app rules land (ticket 07).
    fn begin(&self) {
        self.inserter.begin(Target::default(), matches!(self.config.polish, Polish::On { .. }));
    }

    fn toggle(&self) {
        let result = self.engine().and_then(|engine| match engine.as_ref() {
            Some(e) => lock(&self.session).toggle(e, self.desktop.as_ref()),
            None => Err(BoloError::Stopped),
        });
        match result {
            Ok(Toggled::Started { app }) => {
                self.begin();
                tracing::info!(%app, "dictation started");
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

    /// What the push-to-talk key asks for (on the hook's command thread).
    fn push_to_talk(&self, command: Command) {
        let result = self.engine().and_then(|engine| {
            let Some(e) = engine.as_ref() else { return Err(BoloError::Stopped) };
            let session = lock(&self.session);
            match command {
                Command::Start => session.press(e, self.desktop.as_ref()).map(|t| {
                    if matches!(t, Toggled::Started { .. }) {
                        self.begin();
                    }
                    tracing::info!(?t, "push-to-talk held");
                }),
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
        }
        tracing::info!(paused, "pause");
        self.emit.emit(AppEvent::Paused { paused });
    }

    fn install_hook(&self, app: &AppHandle) {
        let mut hook = lock(&self.hook);
        if hook.is_some() {
            return;
        }
        match KeyHook::install(self.push_to_talk, self.flags.clone(), PushToTalk(app.clone())) {
            Ok(h) => {
                tracing::info!(key = ?self.push_to_talk, "push-to-talk on");
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

    fn toggle_paused(&self, app: &AppHandle) {
        let paused = lock(&self.session).paused();
        self.set_paused(!paused, app);
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

/// Show the main window on a page: `debug`, `settings` or `history`.
fn show_window(app: &AppHandle, page: &str) {
    let Some(window) = app.get_webview_window(MAIN_WINDOW) else { return };
    let _ = window.eval(format!("location.hash = '#{page}'"));
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_focus();
}

fn init_logging() {
    use tracing_subscriber::EnvFilter;
    use tracing_subscriber::fmt::writer::MakeWriterExt;

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    // %LOCALAPPDATA%\Bolo\data\logs, next to the models (ADR-007).
    let file = bolo_models::default_models_root()
        .and_then(|m| m.parent().map(|d| d.join("logs")))
        .and_then(|dir| std::fs::create_dir_all(&dir).ok().map(|()| dir.join("desktop.log")))
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
    let menu = Menu::with_items(
        app,
        &[
            &toggle,
            &pause,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?,
            &MenuItem::with_id(app, "history", "History…", true, None::<&str>)?,
            &MenuItem::with_id(app, "debug", "Debug window", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "quit", "Quit Bolo", true, None::<&str>)?,
        ],
    )?;
    let mut tray = TrayIconBuilder::with_id("bolo").tooltip("Bolo").menu(&menu).show_menu_on_left_click(true);
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.on_menu_event(|app, event| {
        let bolo = app.state::<Bolo>();
        match event.id.as_ref() {
            "toggle" => bolo.toggle(),
            "pause" => bolo.toggle_paused(app),
            "settings" => show_window(app, "settings"),
            "history" => show_window(app, "history"),
            "debug" => show_window(app, "debug"),
            "quit" => {
                // Let go of the keyboard, close the mic and unload the model first.
                bolo.remove_hook();
                drop(lock(&bolo.engine).take());
                app.exit(0);
            }
            _ => {}
        }
    })
    .build(app)?;

    let emit: Arc<dyn Emit> = Arc::new(TauriEmit { app: app.handle().clone(), tray: TrayItems { toggle, pause } });
    let models_dir = bolo_models::default_models_root().unwrap_or_default();
    let (config, config_error) = match config::engine_config(|k| std::env::var(k).ok(), models_dir.clone()) {
        Ok(c) => (c, None),
        Err(e) => {
            tracing::error!(%e, "bad BOLO_* setting; using the defaults");
            (EngineConfig { models_dir, ..Default::default() }, Some(e))
        }
    };
    tracing::info!(engines = ?config.engines, language = ?config.language, models = %config.models_dir.display(), "engine config");
    // Only the key comes from settings so far; the rest arrives with tickets 07 and 09.
    let push_to_talk = match Settings::default_path().and_then(|p| Settings::load(&p)) {
        Ok(loaded) => loaded.settings.push_to_talk,
        Err(e) => {
            tracing::warn!(%e, "could not read the settings; push-to-talk is Caps Lock");
            PushToTalkKey::default()
        }
    };
    let desktop: Arc<dyn Desktop> = Arc::new(WinDesktop);
    let flags = Arc::new(Flags::default());
    let hook_thread = Arc::new(AtomicU32::new(0));
    let thread = hook_thread.clone();
    // The pending swap's mouse hook comes and goes with `Flags::watching`.
    let inserter = Inserter::new(desktop.clone(), Arc::new(ThreadTimer), flags.clone(), move || {
        keyhook::watch_changed(thread.load(Ordering::Acquire));
    });
    let bolo = Bolo {
        config,
        engine: Mutex::new(None),
        session: Mutex::new(Session::default()),
        desktop,
        inserter,
        emit,
        startup_error: Mutex::new(config_error),
        flags,
        push_to_talk,
        hook: Mutex::new(None),
        hook_thread,
    };
    if let Err(e) = bolo.engine() {
        tracing::error!(%e, "the engine could not start; it will be retried on the next dictation");
        lock(&bolo.startup_error).get_or_insert(e.to_string());
    }
    app.manage(bolo);
    app.state::<Bolo>().install_hook(app.handle());
    Ok(())
}

/// Run the app until the user quits from the tray.
pub fn run() {
    init_logging();
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "Bolo desktop starting");
    let app = tauri::Builder::default()
        // First, so a second launch exits before it builds anything.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_window(app, "debug")))
        .invoke_handler(tauri::generate_handler![status, toggle_dictation])
        .setup(setup)
        .on_window_event(|window, event| {
            // Closing a window hides it; Bolo keeps running in the tray.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
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
