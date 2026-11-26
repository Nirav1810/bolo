//! Whole dictations through the `Bolo` object with audio pushed in as PCM16 frames, the
//! way Kotlin drives it, on a fake speech engine (no model, no network), with the
//! settings, history and keys in a temp directory.

use std::collections::HashMap;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use bolo_ffi::{
    AppProfileEntry, Bolo, BoloListener, BoloPaths, BoloSettings, Dictation, ErrorKind, FfiError, FieldKind, Format,
    Language, SecretError, SecretStore, State,
};
use bolo_stt::{SttEngine, SttOptions, Transcript};

/// Returns fixed text for any audio, and records how much audio it was given.
struct FakeStt {
    heard: Arc<Mutex<usize>>,
}

#[async_trait]
impl SttEngine for FakeStt {
    fn id(&self) -> &str {
        "fake"
    }
    async fn transcribe(&self, audio: &[f32], _opts: &SttOptions) -> bolo_stt::Result<Transcript> {
        *self.heard.lock().unwrap() += audio.len();
        Ok(Transcript { text: "testing one two three".into(), engine_id: "fake".into(), ..Transcript::default() })
    }
}

/// What Kotlin's Keystore store does, in memory.
#[derive(Default)]
struct MemoryStore(Mutex<HashMap<String, String>>);

impl SecretStore for MemoryStore {
    fn get(&self, key: String) -> Result<Option<String>, SecretError> {
        Ok(self.0.lock().unwrap().get(&key).cloned())
    }
    fn set(&self, key: String, value: String) -> Result<(), SecretError> {
        self.0.lock().unwrap().insert(key, value);
        Ok(())
    }
    fn delete(&self, key: String) -> Result<bool, SecretError> {
        Ok(self.0.lock().unwrap().remove(&key).is_some())
    }
}

enum Event {
    Final(Dictation),
    Error(ErrorKind, String),
    State(State),
    Settings(BoloSettings),
}

struct Recorder(Mutex<Sender<Event>>);

impl Recorder {
    fn send(&self, e: Event) {
        let _ = self.0.lock().unwrap().send(e);
    }
}

impl BoloListener for Recorder {
    fn on_level(&self, _rms: f32) {}
    fn on_partial(&self, _text: String) {}
    fn on_final(&self, d: Dictation) {
        self.send(Event::Final(d));
    }
    fn on_polished(&self, _d: Dictation) {}
    fn on_state(&self, s: State) {
        self.send(Event::State(s));
    }
    fn on_error(&self, kind: ErrorKind, message: String) {
        self.send(Event::Error(kind, message));
    }
    fn on_settings_changed(&self, settings: BoloSettings) {
        self.send(Event::Settings(settings));
    }
}

fn paths(dir: &Path) -> BoloPaths {
    let s = |p: &str| dir.join(p).to_string_lossy().into_owned();
    BoloPaths { config_dir: s("files"), data_dir: s("files"), models_dir: s("no_backup/models") }
}

struct Rig {
    bolo: Arc<Bolo>,
    events: Receiver<Event>,
    heard: Arc<Mutex<usize>>,
}

fn open(dir: &Path, store: &Arc<MemoryStore>) -> Rig {
    let (tx, events) = mpsc::channel();
    let heard = Arc::new(Mutex::new(0));
    let stt = Arc::new(FakeStt { heard: heard.clone() });
    let bolo = Bolo::with_stt(paths(dir), store.clone(), Arc::new(Recorder(Mutex::new(tx))), stt).expect("opens");
    Rig { bolo, events, heard }
}

/// A `Bolo` on fresh files, its engine set to the fake.
fn rig(dir: &Path) -> Rig {
    let r = open(dir, &Arc::default());
    let settings = BoloSettings { engines: vec!["fake".into()], language: Language::English, ..r.bolo.settings() };
    r.bolo.save_settings(settings).unwrap();
    r
}

/// 20 ms of a quiet tone as little-endian PCM16 bytes, as `AudioRecord` fills a ByteArray.
fn frame(n: usize) -> Vec<u8> {
    (0..320).flat_map(|i| ((((n * 320 + i) as f32 * 0.05).sin() * 3000.0) as i16).to_le_bytes()).collect()
}

/// Dictates one second of tone into `package` and returns the final text.
fn dictate(r: &Rig, package: &str) -> Dictation {
    r.bolo.start(package.into(), FieldKind::Normal).expect("starts");
    assert_eq!(r.bolo.state(), State::Listening);
    for n in 0..50 {
        r.bolo.push_audio(frame(n));
        std::thread::sleep(Duration::from_millis(2));
    }
    std::thread::sleep(Duration::from_millis(100));
    r.bolo.stop().expect("stops");
    loop {
        match r.events.recv_timeout(Duration::from_secs(10)).expect("an event within 10 s") {
            Event::Final(d) => return d,
            Event::Error(kind, msg) => panic!("dictation failed: {kind:?} {msg}"),
            Event::State(_) | Event::Settings(_) => {}
        }
    }
}

#[test]
fn pushed_frames_become_text_and_history() {
    let dir = tempfile::tempdir().unwrap();
    let r = rig(dir.path());
    let d = dictate(&r, "com.example.notes");
    assert_eq!(d.plain, "Testing one two three.");
    assert_eq!(d.engine_id, "fake");
    assert_eq!(d.language, Language::English);
    assert!(*r.heard.lock().unwrap() > 0, "the engine got the pushed audio");
    assert_eq!(r.bolo.dropped_samples(), 0);

    let [row] = r.bolo.history(10).unwrap().try_into().unwrap();
    assert_eq!((row.app.as_str(), row.text.as_str()), ("com.example.notes", "Testing one two three."));
    assert_eq!((row.language.as_str(), row.engine.as_str(), row.polished), ("english", "fake", None));
    assert!(dir.path().join("files/history.sqlite").exists());
}

#[test]
fn audio_pushed_before_start_is_not_heard() {
    let dir = tempfile::tempdir().unwrap();
    let r = rig(dir.path());
    // A second of audio while no dictation has opened the mic.
    for n in 0..50 {
        r.bolo.push_audio(frame(n));
    }
    r.bolo.start("com.example.notes".into(), FieldKind::Normal).expect("starts");
    r.bolo.stop().expect("stops");
    // Wait for the dictation to end one way or the other.
    while let Ok(event) = r.events.recv_timeout(Duration::from_secs(10)) {
        if matches!(event, Event::Final(_) | Event::Error(..) | Event::State(State::Done | State::Idle)) {
            break;
        }
    }
    assert!(*r.heard.lock().unwrap() < 16_000, "idle audio leaked into the dictation");
}

#[test]
fn sensitive_fields_never_record() {
    let dir = tempfile::tempdir().unwrap();
    let r = rig(dir.path());
    for field in [FieldKind::Password, FieldKind::Incognito] {
        assert!(matches!(r.bolo.start("com.example.bank".into(), field), Err(FfiError::SensitiveField)));
        assert_eq!(r.bolo.state(), State::Idle, "the mic never opened");
        for n in 0..10 {
            r.bolo.push_audio(frame(n));
        }
    }
    assert_eq!(*r.heard.lock().unwrap(), 0);
    assert!(r.bolo.history(10).unwrap().is_empty());
}

#[test]
fn the_app_profile_decides_the_format_by_package() {
    let dir = tempfile::tempdir().unwrap();
    let r = rig(dir.path());
    let target = r.bolo.start("com.Slack".into(), FieldKind::Normal).unwrap();
    assert_eq!((target.app_name.as_str(), target.format, target.polish), ("Slack", Format::Markdown, false));
    r.bolo.cancel();

    let mut s = r.bolo.settings();
    s.polish = true;
    s.apps = vec![AppProfileEntry {
        package: "com.slack".into(),
        name: Some("Work chat".into()),
        polish: None,
        tone: None,
        format: Some(Format::Plain),
    }];
    r.bolo.save_settings(s).unwrap();
    let target = r.bolo.start("com.slack".into(), FieldKind::Normal).unwrap();
    assert_eq!((target.app_name.as_str(), target.format), ("Work chat", Format::Plain));
    assert!(!target.polish, "polish needs the Anthropic key");
    r.bolo.cancel();

    r.bolo.set_api_key("anthropic".into(), "sk-ant-test-1234".into()).unwrap();
    let target = r.bolo.start("com.whatsapp".into(), FieldKind::Normal).unwrap();
    assert_eq!((target.app_name.as_str(), target.format, target.polish), ("WhatsApp", Format::Plain, true));
    r.bolo.cancel();

    let slack = r.bolo.apps().into_iter().find(|a| a.package == "com.slack").unwrap();
    assert!(slack.custom && slack.bundled);
    assert_eq!((slack.name.as_str(), slack.format), ("Work chat", Format::Plain));
}

#[test]
fn a_save_reaches_every_bolo_on_the_file_and_the_engine() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(MemoryStore::default());
    let a = open(dir.path(), &store);
    let b = open(dir.path(), &store);
    let other_dir = tempfile::tempdir().unwrap();
    let elsewhere = open(other_dir.path(), &store);

    let config = a.bolo.engine_config();
    assert_eq!((config.idle_unload_secs, config.mic_warm_secs), (20, 30), "Android's budgets");
    assert_eq!(config.models_dir, dir.path().join("no_backup/models"));

    let saved = a
        .bolo
        .save_settings(BoloSettings { engines: vec![" fake ".into()], tone: "Casual".into(), ..a.bolo.settings() })
        .unwrap();
    assert_eq!((saved.engines.as_slice(), saved.tone.as_str()), (["fake".to_owned()].as_slice(), "casual"));
    for r in [&a, &b] {
        let Ok(Event::Settings(s)) = r.events.recv_timeout(Duration::from_secs(1)) else { panic!("no settings event") };
        assert_eq!(s, saved);
        assert_eq!(r.bolo.settings(), saved);
        assert_eq!(r.bolo.engine_config().engines, ["fake"]);
    }
    assert!(elsewhere.events.try_recv().is_err(), "another file's Bolo isn't told");
    assert!(dir.path().join("files/settings.toml").exists());

    // A bad record changes nothing.
    let bad = BoloSettings {
        apps: vec![AppProfileEntry {
            package: "com.slack".into(),
            name: None,
            polish: None,
            tone: Some("shouty".into()),
            format: None,
        }],
        ..saved.clone()
    };
    assert!(matches!(a.bolo.save_settings(bad), Err(FfiError::Invalid { .. })));
    assert_eq!(b.bolo.settings(), saved);

    // Keys set through one are read by the other.
    assert_eq!(a.bolo.set_api_key("deepgram".into(), "dg-key-abcd".into()).unwrap().as_deref(), Some("abcd"));
    assert_eq!(b.bolo.api_key_hint("deepgram".into()).as_deref(), Some("abcd"));
    assert_eq!(b.bolo.engine_config().api_keys.get("deepgram"), Some("dg-key-abcd"));
    assert_eq!(store.get("bolo/deepgram".into()).unwrap().as_deref(), Some("dg-key-abcd"));
    assert!(matches!(a.bolo.set_api_key("openai".into(), "k".into()), Err(FfiError::Invalid { .. })));
    a.bolo.set_api_key("deepgram".into(), " ".into()).unwrap();
    assert_eq!(b.bolo.api_key_hint("deepgram".into()), None);
}

#[test]
fn another_process_s_save_is_picked_up_on_reload() {
    let dir = tempfile::tempdir().unwrap();
    let r = rig(dir.path());
    while r.events.try_recv().is_ok() {}
    assert!(!r.bolo.reload_settings().unwrap(), "nothing changed");
    let file = dir.path().join("files/settings.toml");
    let text = std::fs::read_to_string(&file).unwrap().replace("language = \"english\"", "language = \"hinglish\"");
    // Make sure the modification time moves even on a coarse clock.
    std::thread::sleep(Duration::from_millis(20));
    std::fs::write(&file, text).unwrap();
    let later = std::time::SystemTime::now() + Duration::from_secs(2);
    std::fs::File::options().write(true).open(&file).unwrap().set_modified(later).unwrap();
    assert!(r.bolo.reload_settings().unwrap());
    assert_eq!(r.bolo.settings().language, Language::Hinglish);
    assert!(
        matches!(r.events.recv_timeout(Duration::from_secs(1)), Ok(Event::Settings(s)) if s.language == Language::Hinglish)
    );
    assert!(!r.bolo.reload_settings().unwrap());
}

#[test]
fn history_can_be_searched_deleted_and_cleared_and_respects_the_switch() {
    let dir = tempfile::tempdir().unwrap();
    let r = rig(dir.path());
    dictate(&r, "com.whatsapp");
    dictate(&r, "com.google.android.gm");
    let rows = r.bolo.history(10).unwrap();
    assert_eq!(rows.iter().map(|h| h.app.as_str()).collect::<Vec<_>>(), ["com.google.android.gm", "com.whatsapp"]);
    assert_eq!(r.bolo.search_history(" one two ".into(), 10).unwrap().len(), 2);
    assert!(r.bolo.search_history("nothing like it".into(), 10).unwrap().is_empty());
    assert!(r.bolo.delete_history(rows[0].id).unwrap());
    assert!(!r.bolo.delete_history(rows[0].id).unwrap());
    assert_eq!(r.bolo.history(10).unwrap().len(), 1);
    r.bolo.clear_history().unwrap();
    assert!(r.bolo.history(10).unwrap().is_empty());

    r.bolo.save_settings(BoloSettings { history: false, ..r.bolo.settings() }).unwrap();
    dictate(&r, "com.whatsapp");
    assert!(r.bolo.history(10).unwrap().is_empty(), "history off: nothing written");
}
