//! UniFFI bindings for the Android app (ADR-0023).
//!
//! Kotlin sees one object, [`Bolo`], and two foreign traits: [`BoloListener`] for events
//! and [`SecretStore`] for API keys. `Bolo` owns the engine and the desktop's store
//! (`bolo-store`): `settings.toml` v2, history and the usage meter, in the directories
//! Kotlin passes in ([`BoloPaths`]). Audio is recorded by Kotlin (`AudioRecord`,
//! `VOICE_RECOGNITION`) and pushed in with [`Bolo::push_audio`]; the engine drains it
//! through [`push::PushSource`].
//!
//! The keyboard and the settings app each hold a `Bolo` on the same files. A save
//! through one is adopted by every `Bolo` in the process, and each tells its listener
//! (`on_settings_changed`); [`Bolo::reload_settings`] picks up a file written by another
//! process.
//!
//! Every listener call comes from a Rust thread (or, for `on_settings_changed`, the
//! thread that saved): Kotlin posts it to the main thread.

use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Once, PoisonError, Weak};
use std::time::SystemTime;

use bolo_core::{
    ANTHROPIC, BoloError, DictationState, Engine, EngineConfig, EngineEvents, FormattedOutput, Polish, Usage,
};
use bolo_store::apps::{self, AppKey, AppRules, PolishPrefs};
use bolo_store::{History, Journal, Paths, Secrets, Settings, StoreError};

pub mod push;
pub mod records;
pub mod secrets;

use push::PushRing;
pub use records::{
    AppEntry, AppProfileEntry, BoloSettings, Format, HistoryEntry, Language, Script, SnippetEntry,
};
pub use secrets::{SecretError, SecretStore};

uniffi::setup_scaffolding!();

/// Android's budgets (ADR-0022): the model unloads 20 s after the last dictation, and
/// the mic stays warm for 30 s while the keyboard is shown.
const IDLE_UNLOAD_SECS: u32 = 20;
const MIC_WARM_SECS: u32 = 30;

const MS_PER_DAY: i64 = 86_400_000;

/// Where Bolo keeps its files, from Kotlin.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BoloPaths {
    /// `settings.toml` goes here: `filesDir`.
    pub config_dir: String,
    /// `history.sqlite` goes here: `filesDir`.
    pub data_dir: String,
    /// The models root: `noBackupFilesDir/models`.
    pub models_dir: String,
}

impl From<BoloPaths> for Paths {
    fn from(p: BoloPaths) -> Self {
        Self {
            config_dir: PathBuf::from(p.config_dir),
            data_dir: PathBuf::from(p.data_dir),
            models_dir: PathBuf::from(p.models_dir),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum State {
    Idle,
    Listening,
    Processing,
    Done,
}

impl From<DictationState> for State {
    fn from(s: DictationState) -> Self {
        match s {
            DictationState::Idle => State::Idle,
            DictationState::Listening => State::Listening,
            DictationState::Processing => State::Processing,
            DictationState::Done => State::Done,
        }
    }
}

/// The text field a dictation would go into, as the keyboard reads its `EditorInfo`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum FieldKind {
    /// An ordinary text field.
    Normal,
    /// A password field (`TYPE_TEXT_VARIATION_*PASSWORD`, `TYPE_NUMBER_VARIATION_PASSWORD`).
    Password,
    /// The app asked for no learning (`IME_FLAG_NO_PERSONALIZED_LEARNING`): incognito tabs
    /// and the like.
    Incognito,
}

impl FieldKind {
    /// Bolo never records into it, and never writes it into history (ADR-0022).
    pub fn is_sensitive(self) -> bool {
        !matches!(self, Self::Normal)
    }
}

/// What a dictation that just started will do, for the keyboard.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct DictationTarget {
    /// The app's name, as polish is told it.
    pub app_name: String,
    /// The text to commit: `plain` from `on_final`, or `markdown` for [`Format::Markdown`].
    pub format: Format,
    /// Polish will run, so an `on_polished` may follow `on_final`.
    pub polish: bool,
}

/// One dictation's text, as `on_final` or `on_polished` delivers it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Dictation {
    pub plain: String,
    pub markdown: String,
    pub engine_id: String,
    pub language: Language,
    /// Stop → recognized text.
    pub stt_ms: f64,
    /// Stop → text ready to insert.
    pub total_ms: f64,
}

impl From<FormattedOutput> for Dictation {
    fn from(o: FormattedOutput) -> Self {
        Self {
            plain: o.plain,
            markdown: o.markdown,
            engine_id: o.engine_id,
            language: o.language.into(),
            stt_ms: o.timings.stt_ms,
            total_ms: o.timings.total_ms,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ErrorKind {
    Audio,
    Stt,
    ModelMissing,
    Config,
    Busy,
    Stopped,
}

/// A failed call. (The text is `detail`: Kotlin exceptions already have a `message`.)
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum FfiError {
    /// The engine refused or failed.
    #[error("{detail}")]
    Engine { kind: ErrorKind, detail: String },
    /// The settings file, the history database or the key store failed.
    #[error("{detail}")]
    Store { detail: String },
    /// A settings value was refused (an unknown tone, a bad package name, ...).
    #[error("{detail}")]
    Invalid { detail: String },
    /// The field is a password or incognito field: Bolo doesn't listen there.
    #[error("Bolo doesn't dictate into password or incognito fields")]
    SensitiveField,
}

impl From<BoloError> for FfiError {
    fn from(e: BoloError) -> Self {
        FfiError::Engine { kind: kind_of(&e), detail: e.to_string() }
    }
}

impl From<StoreError> for FfiError {
    fn from(e: StoreError) -> Self {
        match e {
            StoreError::UnknownProvider(_) => FfiError::Invalid { detail: e.to_string() },
            e => FfiError::Store { detail: e.to_string() },
        }
    }
}

fn kind_of(e: &BoloError) -> ErrorKind {
    match e {
        BoloError::Audio(_) => ErrorKind::Audio,
        BoloError::Stt(_) => ErrorKind::Stt,
        BoloError::ModelMissing(_) => ErrorKind::ModelMissing,
        BoloError::Config(_) => ErrorKind::Config,
        BoloError::Busy => ErrorKind::Busy,
        BoloError::Stopped => ErrorKind::Stopped,
    }
}

/// Engine and settings events, implemented in Kotlin.
#[uniffi::export(with_foreign)]
pub trait BoloListener: Send + Sync {
    /// Input level (RMS, 0..1) for the waveform, ~30 Hz while listening.
    fn on_level(&self, rms: f32);
    /// The words so far; replaces the previous partial.
    fn on_partial(&self, text: String);
    /// The instant text, as soon as recognition finishes.
    fn on_final(&self, dictation: Dictation);
    /// With polish on: the polished text that replaces the last `on_final`.
    fn on_polished(&self, dictation: Dictation);
    fn on_state(&self, state: State);
    fn on_error(&self, kind: ErrorKind, message: String);
    /// The settings changed (saved through any `Bolo` in this process, or reloaded from
    /// the file): the settings as they are now.
    fn on_settings_changed(&self, settings: BoloSettings);
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Engine events: history and usage are journaled here, then passed on to Kotlin.
struct Events {
    listener: Arc<dyn BoloListener>,
    journal: Arc<Journal>,
    retention_days: Arc<AtomicU32>,
    last_prune_ms: Arc<AtomicI64>,
}

impl EngineEvents for Events {
    fn on_level(&self, rms: f32) {
        self.listener.on_level(rms);
    }
    fn on_partial(&self, text: String) {
        self.listener.on_partial(text);
    }
    fn on_final(&self, out: FormattedOutput) {
        self.journal.finished(&out);
        prune_daily(&self.journal, &self.retention_days, &self.last_prune_ms);
        self.listener.on_final(out.into());
    }
    fn on_polished(&self, out: FormattedOutput) {
        // Whether the keyboard could replace the text is its call (ticket 06).
        self.journal.polished(&out, "delivered");
        self.listener.on_polished(out.into());
    }
    fn on_error(&self, err: BoloError) {
        self.listener.on_error(kind_of(&err), err.to_string());
    }
    fn on_state(&self, state: DictationState) {
        self.listener.on_state(state.into());
    }
    fn on_usage(&self, usage: Usage) {
        self.journal.usage(&usage);
    }
}

/// Prunes history now, unless that was done in the last day.
fn prune_daily(journal: &Journal, retention_days: &AtomicU32, last_prune_ms: &AtomicI64) {
    let now = bolo_store::history::now_ms();
    if now - last_prune_ms.load(Ordering::Relaxed) < MS_PER_DAY {
        return;
    }
    last_prune_ms.store(now, Ordering::Relaxed);
    let days = retention_days.load(Ordering::Relaxed);
    match journal.with_history(|h| h.prune(days, now)) {
        Some(Ok(n)) if n > 0 => tracing::info!(n, days, "pruned old history"),
        Some(Err(e)) => tracing::warn!(%e, "could not prune history"),
        _ => {}
    }
}

/// The engine config for `settings` on Android: the models in `paths.models_dir` (the
/// settings' own `models_dir` is the desktop's) and Android's budgets.
fn engine_config(settings: &Settings, secrets: &Secrets, paths: &Paths) -> EngineConfig {
    EngineConfig {
        models_dir: paths.models_dir.clone(),
        idle_unload_secs: IDLE_UNLOAD_SECS,
        mic_warm_secs: MIC_WARM_SECS,
        ..settings.engine_config(secrets)
    }
}

/// The settings file's modification time, to notice another process's save.
fn modified(paths: &Paths) -> Option<SystemTime> {
    std::fs::metadata(paths.settings_file()).and_then(|m| m.modified()).ok()
}

/// Every open `Bolo` in the process, so a save reaches them all.
static OPEN: Mutex<Vec<Weak<Inner>>> = Mutex::new(Vec::new());

/// The `Bolo`s open on `paths`' settings file.
fn open_on(paths: &Paths) -> Vec<Arc<Inner>> {
    let file = paths.settings_file();
    lock(&OPEN).iter().filter_map(Weak::upgrade).filter(|i| i.paths.settings_file() == file).collect()
}

struct Inner {
    engine: Engine,
    ring: Arc<PushRing>,
    paths: Paths,
    rules: AppRules,
    settings: Mutex<Settings>,
    /// The file's modification time when this `Bolo` last read or wrote it.
    seen: Mutex<Option<SystemTime>>,
    secrets: Mutex<Secrets>,
    journal: Arc<Journal>,
    retention_days: Arc<AtomicU32>,
    listener: Arc<dyn BoloListener>,
}

impl Inner {
    fn prefs(&self) -> PolishPrefs {
        let settings = lock(&self.settings);
        PolishPrefs {
            enabled: settings.polish.enabled,
            tone: settings.polish.tone.clone(),
            dictionary: settings.dictionary_terms(),
            apps: settings.apps.clone(),
            has_key: lock(&self.secrets).has(ANTHROPIC),
        }
    }

    fn config(&self) -> EngineConfig {
        engine_config(&lock(&self.settings), &lock(&self.secrets), &self.paths)
    }

    /// Takes `new` as the settings: the engine, history and the listener follow.
    fn adopt(&self, new: Settings) {
        *lock(&self.seen) = modified(&self.paths);
        let old = std::mem::replace(&mut *lock(&self.settings), new.clone());
        let changes = bolo_store::form::changes(&old, &new);
        if changes.engine {
            // The model reloads at the next dictation; the keyboard never warms it
            // before a mic tap (ADR-0022), so `changes.warm` is ignored.
            self.engine.update_config(self.config());
        } else if changes.snippets {
            self.engine.set_snippets(new.snippet_list());
        }
        if changes.history {
            self.journal.with_history(|h| h.set_enabled(new.history.enabled));
        }
        self.retention_days.store(new.history.retention_days, Ordering::Relaxed);
        if changes.retention {
            prune(&self.journal, new.history.retention_days);
        }
        if old != new {
            self.listener.on_settings_changed(BoloSettings::of(&new));
        }
    }

    /// The keys changed in the store: read them again.
    fn reload_keys(&self) {
        let store = lock(&self.secrets).store();
        match Secrets::with_store(store) {
            Ok(secrets) => *lock(&self.secrets) = secrets,
            Err(e) => tracing::warn!(%e, "could not read the API keys again"),
        }
        self.engine.update_config(self.config());
    }
}

fn prune(journal: &Journal, days: u32) {
    match journal.with_history(|h| h.prune(days, bolo_store::history::now_ms())) {
        Some(Ok(n)) if n > 0 => tracing::info!(n, days, "pruned old history"),
        Some(Err(e)) => tracing::warn!(%e, "could not prune history"),
        _ => {}
    }
}

/// The dictation engine with its audio fed from Kotlin, and the settings, history and
/// keys it runs on.
#[derive(uniffi::Object)]
pub struct Bolo {
    inner: Arc<Inner>,
}

#[uniffi::export]
impl Bolo {
    /// Opens the settings (`config_dir/settings.toml`; a missing file is the defaults,
    /// a bad one is backed up and replaced), history (`data_dir/history.sqlite`, pruned
    /// now) and the keys in `secret_store`, and starts the engine. The model loads at the
    /// first dictation.
    #[uniffi::constructor]
    pub fn new(
        paths: BoloPaths,
        secret_store: Arc<dyn SecretStore>,
        listener: Arc<dyn BoloListener>,
    ) -> Result<Arc<Self>, FfiError> {
        init_logging();
        Self::open(paths.into(), secret_store, listener, None)
    }

    /// One frame of mono 16-bit little-endian PCM at 16 kHz, as `AudioRecord.read` fills
    /// a `ByteArray` (~20 ms, 640 bytes). Dropped while the engine's mic is closed.
    pub fn push_audio(&self, pcm16le: Vec<u8>) {
        self.inner.ring.push_pcm16le(&pcm16le);
    }

    /// Load the model ahead of the first dictation.
    pub fn prewarm(&self) {
        self.inner.engine.prewarm();
    }

    /// Start a dictation into the app with this package name, as the app's profile and
    /// the bundled rules say (ADR-0020, ADR-0022). Returns once the mic is open, so audio
    /// pushed from now on is heard. A sensitive field is refused with
    /// [`FfiError::SensitiveField`]: nothing is recorded and nothing goes into history.
    pub fn start(&self, package_name: String, field: FieldKind) -> Result<DictationTarget, FfiError> {
        if field.is_sensitive() {
            return Err(FfiError::SensitiveField);
        }
        let inner = &self.inner;
        let plan = apps::plan(&AppKey::package(&package_name), "", &inner.rules, &inner.prefs());
        let ctx = plan.resolved.ctx;
        let target = DictationTarget {
            app_name: plan.resolved.app_name,
            format: ctx.format.into(),
            polish: matches!(plan.polish, Polish::On { .. }),
        };
        inner.engine.set_polish(plan.polish);
        inner.journal.begin(&ctx.app_id, ctx.format);
        inner.engine.start_dictation(ctx)?;
        Ok(target)
    }

    /// Finish; the text arrives through `on_final`.
    pub fn stop(&self) -> Result<(), FfiError> {
        Ok(self.inner.engine.stop_dictation()?)
    }

    /// Throw the running dictation (or its polish) away.
    pub fn cancel(&self) {
        self.inner.engine.cancel_dictation();
    }

    /// Unload the model now (`onTrimMemory`).
    pub fn release(&self) {
        self.inner.engine.release_resources();
    }

    pub fn state(&self) -> State {
        self.inner.engine.state().into()
    }

    /// Samples dropped because the engine fell behind, for diagnostics.
    pub fn dropped_samples(&self) -> u64 {
        self.inner.ring.dropped()
    }

    /// The settings as they are now.
    pub fn settings(&self) -> BoloSettings {
        BoloSettings::of(&lock(&self.inner.settings))
    }

    /// Saves `settings`, tidied the way the desktop's settings window tidies them, and
    /// every `Bolo` on this file adopts them (`on_settings_changed`). The settings as
    /// saved. Settings the record doesn't cover (the desktop's, unknown keys) are kept.
    pub fn save_settings(&self, settings: BoloSettings) -> Result<BoloSettings, FfiError> {
        let inner = &self.inner;
        let new = {
            let current = lock(&inner.settings);
            settings.apply(&current).map_err(|detail| FfiError::Invalid { detail })?
        };
        new.save(&inner.paths.settings_file())?;
        for bolo in open_on(&inner.paths) {
            bolo.adopt(new.clone());
        }
        Ok(BoloSettings::of(&new))
    }

    /// Reads the settings file again if it changed since this `Bolo` last read or wrote
    /// it (another process saved). Whether it had.
    pub fn reload_settings(&self) -> Result<bool, FfiError> {
        let inner = &self.inner;
        let now = modified(&inner.paths);
        if now == *lock(&inner.seen) {
            return Ok(false);
        }
        let loaded = Settings::load(&inner.paths.settings_file())?;
        inner.adopt(loaded.settings);
        Ok(true)
    }

    /// The Apps page: the Android apps the bundled rules know, then the ones the user
    /// added, with each field resolved.
    pub fn apps(&self) -> Vec<AppEntry> {
        records::app_entries(&self.inner.rules, &lock(&self.inner.settings))
    }

    /// Stores `provider`'s API key (blank deletes it); every `Bolo` in the process reads
    /// the keys again. The key's hint (its last four characters).
    pub fn set_api_key(&self, provider: String, key: String) -> Result<Option<String>, FfiError> {
        let hint = {
            let mut secrets = lock(&self.inner.secrets);
            secrets.set(&provider, &key)?;
            secrets.hint(&provider)
        };
        tracing::info!(provider, stored = hint.is_some(), "API key changed");
        for bolo in open_on(&self.inner.paths) {
            bolo.reload_keys();
        }
        Ok(hint)
    }

    /// What the settings show for a stored key: its last four characters (empty for a
    /// short key). `None` when there's no key.
    pub fn api_key_hint(&self, provider: String) -> Option<String> {
        lock(&self.inner.secrets).hint(&provider)
    }

    /// The newest `limit` dictations, newest first.
    pub fn history(&self, limit: u32) -> Result<Vec<HistoryEntry>, FfiError> {
        self.search_history(String::new(), limit)
    }

    /// Dictations whose text contains `query` (case-insensitive for ASCII), newest first.
    pub fn search_history(&self, query: String, limit: u32) -> Result<Vec<HistoryEntry>, FfiError> {
        let rows = self.inner.journal.with_history(|h| h.search(query.trim(), limit as usize));
        Ok(rows.transpose()?.unwrap_or_default().into_iter().map(HistoryEntry::from).collect())
    }

    /// Deletes one dictation. Whether there was one.
    pub fn delete_history(&self, id: i64) -> Result<bool, FfiError> {
        Ok(self.inner.journal.with_history(|h| h.delete(id)).transpose()?.unwrap_or(false))
    }

    /// Deletes every dictation (the usage meter stays).
    pub fn clear_history(&self) -> Result<(), FfiError> {
        Ok(self.inner.journal.with_history(|h| h.clear()).transpose().map(drop)?)
    }
}

impl Bolo {
    fn open(
        paths: Paths,
        secret_store: Arc<dyn SecretStore>,
        listener: Arc<dyn BoloListener>,
        stt: Option<Arc<dyn bolo_stt::SttEngine>>,
    ) -> Result<Arc<Self>, FfiError> {
        let loaded = Settings::load(&paths.settings_file())?;
        if let Some(backup) = &loaded.recovered_from {
            tracing::warn!(backup = %backup.display(), "the settings file was bad; using the defaults");
        }
        let settings = loaded.settings;
        let secrets = Secrets::with_store(secrets::ForeignStore::new(secret_store))?;
        let history = match History::open(&paths.history_file()) {
            Ok(mut h) => {
                h.set_enabled(settings.history.enabled);
                Some(h)
            }
            Err(e) => {
                tracing::warn!(%e, "could not open history; dictations won't be recorded");
                None
            }
        };
        let journal = Arc::new(Journal::new(history));
        let retention_days = Arc::new(AtomicU32::new(settings.history.retention_days));
        let last_prune_ms = Arc::new(AtomicI64::new(0));
        prune_daily(&journal, &retention_days, &last_prune_ms);

        let ring = PushRing::new();
        let config = engine_config(&settings, &secrets, &paths);
        let events = Box::new(Events {
            listener: listener.clone(),
            journal: journal.clone(),
            retention_days: retention_days.clone(),
            last_prune_ms,
        });
        let engine = match stt {
            None => Engine::with_audio_source(config.clone(), events, ring.factory())?,
            Some(stt) => Engine::with_stt_engine(config.clone(), events, ring.factory(), stt)?,
        };
        tracing::info!(engines = ?config.engines, language = ?config.language, "engine started");
        let inner = Arc::new(Inner {
            engine,
            ring,
            seen: Mutex::new(modified(&paths)),
            paths,
            rules: AppRules::bundled(),
            settings: Mutex::new(settings),
            secrets: Mutex::new(secrets),
            journal,
            retention_days,
            listener,
        });
        let mut open = lock(&OPEN);
        open.retain(|w| w.strong_count() > 0);
        open.push(Arc::downgrade(&inner));
        Ok(Arc::new(Self { inner }))
    }

    /// A `Bolo` on an injected speech engine, for tests with no model.
    #[doc(hidden)]
    pub fn with_stt(
        paths: BoloPaths,
        secret_store: Arc<dyn SecretStore>,
        listener: Arc<dyn BoloListener>,
        stt: Arc<dyn bolo_stt::SttEngine>,
    ) -> Result<Arc<Self>, FfiError> {
        Self::open(paths.into(), secret_store, listener, Some(stt))
    }

    /// The engine config this `Bolo` runs on, for tests.
    #[doc(hidden)]
    pub fn engine_config(&self) -> EngineConfig {
        self.inner.config()
    }
}

/// Route `tracing` to logcat (tag `bolo`) once per process.
fn init_logging() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        #[cfg(target_os = "android")]
        {
            use tracing_subscriber::layer::SubscriberExt;
            use tracing_subscriber::util::SubscriberInitExt;
            if let Ok(layer) = tracing_android::layer("bolo") {
                let filter = tracing_subscriber::EnvFilter::new("info,bolo=debug");
                let _ = tracing_subscriber::registry().with(filter).with(layer).try_init();
            }
        }
    });
}
