//! Models through `bolo-ffi` (ticket 05): downloads against a local HTTP server (progress
//! throttling, cancel and resume, a bad checksum, the VAD alongside), and the `Bolo`
//! calls the Models page uses (the tier's list, the recommendation, delete, events to
//! every `Bolo` on the same models).

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use bolo_ffi::models::{CancelToken, PROGRESS_INTERVAL, run};
use bolo_ffi::{
    Bolo, BoloListener, BoloPaths, BoloSettings, DeviceTier, Dictation, DownloadPhase, ErrorKind, FfiError, Language,
    ModelDownload, SecretError, SecretStore, State,
};
use bolo_models::{Downloader, ModelEntry};
use bolo_stt::{SttEngine, SttOptions, Transcript};
use sha2::{Digest, Sha256};

const MIB: usize = 1 << 20;

/// Serves files over HTTP/1.1 with `Range: bytes=N-` support, `chunk` bytes at a time
/// with `pause` in between. Records each request's Range header.
struct Server {
    base: String,
    ranges: Arc<Mutex<Vec<Option<String>>>>,
}

impl Server {
    fn start(files: HashMap<&'static str, Vec<u8>>, chunk: usize, pause: Duration) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let ranges = Arc::new(Mutex::new(Vec::new()));
        let files = Arc::new(files);
        let seen = ranges.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let (files, seen) = (files.clone(), seen.clone());
                std::thread::spawn(move || serve(stream, &files, &seen, chunk, pause));
            }
        });
        Self { base, ranges }
    }

    fn url(&self, name: &str) -> String {
        format!("{}/{name}", self.base)
    }

    fn ranges(&self) -> Vec<Option<String>> {
        self.ranges.lock().unwrap().clone()
    }
}

fn serve(
    stream: TcpStream,
    files: &HashMap<&'static str, Vec<u8>>,
    seen: &Mutex<Vec<Option<String>>>,
    chunk: usize,
    pause: Duration,
) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut first = String::new();
    if reader.read_line(&mut first).is_err() {
        return;
    }
    let path = first.split_whitespace().nth(1).unwrap_or("/").trim_start_matches('/').to_owned();
    let mut range = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
            break;
        }
        if let Some((k, v)) = line.split_once(':')
            && k.eq_ignore_ascii_case("range")
        {
            range = Some(v.trim().to_owned());
        }
    }
    seen.lock().unwrap().push(range.clone());
    let mut out = stream;
    let Some(body) = files.get(path.as_str()) else {
        let _ = out.write_all(b"HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n");
        return;
    };
    let from: usize =
        range.as_deref().and_then(|r| r.strip_prefix("bytes=")?.strip_suffix('-')?.parse().ok()).unwrap_or(0);
    let head = if from > 0 {
        format!(
            "HTTP/1.1 206 Partial Content\r\ncontent-length: {}\r\ncontent-range: bytes {from}-{}/{}\r\nconnection: close\r\n\r\n",
            body.len() - from,
            body.len() - 1,
            body.len()
        )
    } else {
        format!("HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n", body.len())
    };
    if out.write_all(head.as_bytes()).is_err() {
        return;
    }
    for part in body[from..].chunks(chunk) {
        if out.write_all(part).is_err() {
            return; // the client went away (a cancel)
        }
        if !pause.is_zero() {
            std::thread::sleep(pause);
        }
    }
    let _ = out.flush();
    let _ = out.read(&mut [0; 1]);
}

fn body(len: usize, seed: u8) -> Vec<u8> {
    (0..len).map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed)).collect()
}

/// A single-file model served at `url`.
fn entry(id: &str, url: &str, body: &[u8]) -> ModelEntry {
    let sha256 = hex::encode(Sha256::digest(body));
    serde_json::from_value(serde_json::json!({
        "id": id, "name": id, "kind": "asr", "family": "offline-moonshine-v2", "streaming": false,
        "languages": ["en"], "hindi_script": null, "status": "available", "min_sherpa_onnx": "2.0.0",
        "license": { "id": "MIT", "redistributable": true, "commercial": true, "url": "https://example.com", "attribution": "test" },
        "download": { "url": url, "sha256": sha256, "size_bytes": body.len(), "archive": null, "root_dir": null },
        "files": { "model": "model.bin" }, "unpacked_size_bytes": body.len(), "min_ram_mb": 1, "tiers": ["android-low"]
    }))
    .unwrap()
}

fn downloader(root: &Path) -> Downloader {
    Downloader::new(bolo_stt::cloud::http_client().unwrap(), root)
}

/// Collects the events `run` reports.
fn recorder() -> (Arc<Mutex<Vec<ModelDownload>>>, impl Fn(ModelDownload) + Sync) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let e = events.clone();
    (events, move |d| e.lock().unwrap().push(d))
}

fn phases(events: &[ModelDownload]) -> Vec<DownloadPhase> {
    let mut v: Vec<DownloadPhase> = events.iter().map(|e| e.phase).collect();
    v.dedup();
    v
}

#[test]
fn a_download_is_verified_installed_and_reported_at_most_ten_times_a_second() {
    let data = body(24 * MIB, 1);
    let server = Server::start(HashMap::from([("m.bin", data.clone())]), 256 * 1024, Duration::from_millis(4));
    let dir = tempfile::tempdir().unwrap();
    let e = entry("test-model", &server.url("m.bin"), &data);
    let (events, report) = recorder();
    let started = Instant::now();
    let installed =
        run(&downloader(dir.path()), &e, None, &CancelToken::default(), PROGRESS_INTERVAL, &report).unwrap();
    let elapsed = started.elapsed();
    assert!(installed);
    assert!(e.is_installed(dir.path()));
    assert_eq!(std::fs::read(dir.path().join("test-model/model.bin")).unwrap(), data);

    let events = events.lock().unwrap();
    assert_eq!(
        phases(&events),
        [DownloadPhase::Downloading, DownloadPhase::Verifying, DownloadPhase::Unpacking, DownloadPhase::Done]
    );
    let downloading: Vec<_> = events.iter().filter(|e| e.phase == DownloadPhase::Downloading).collect();
    // `bolo-models` reports every MiB (24 here); the throttle lets through about one per
    // 100 ms, plus the first and the last byte.
    let allowed = (elapsed.as_millis() / PROGRESS_INTERVAL.as_millis()) as usize + 2;
    assert!(downloading.len() <= allowed.min(23), "{} events in {elapsed:?}", downloading.len());
    assert!(downloading.windows(2).all(|w| w[0].downloaded < w[1].downloaded));
    assert_eq!(downloading.last().unwrap().downloaded, data.len() as u64, "the last byte always passes");
    let done = events.last().unwrap();
    assert_eq!((done.downloaded, done.total, done.error.as_deref()), (data.len() as u64, data.len() as u64, None));
}

#[test]
fn a_cancel_keeps_the_partial_file_and_the_next_download_resumes_from_it() {
    let data = body(8 * MIB, 2);
    // 64 KiB every 10 ms: about 1.3 s for the whole file.
    let server = Server::start(HashMap::from([("m.bin", data.clone())]), 64 * 1024, Duration::from_millis(10));
    let dir = tempfile::tempdir().unwrap();
    let e = entry("test-model", &server.url("m.bin"), &data);
    let cancel = CancelToken::default();
    let (events, report) = recorder();
    let report = |d: ModelDownload| {
        // Cancel once 2 MiB are in.
        if d.phase == DownloadPhase::Downloading && d.downloaded >= 2 * MIB as u64 {
            cancel.cancel();
        }
        report(d);
    };
    let started = Instant::now();
    let installed = run(&downloader(dir.path()), &e, None, &cancel, Duration::ZERO, &report).unwrap();
    assert!(!installed, "cancelled");
    assert!(started.elapsed() < Duration::from_secs(1), "stopped early: {:?}", started.elapsed());
    assert_eq!(events.lock().unwrap().last().unwrap().phase, DownloadPhase::Cancelled);
    assert!(!e.is_installed(dir.path()));
    let partial = bolo_models::partial_bytes(dir.path(), &e);
    assert!(partial >= 2 * MIB as u64 && partial < data.len() as u64, "{partial} bytes kept");

    let (events, report) = recorder();
    assert!(run(&downloader(dir.path()), &e, None, &CancelToken::default(), Duration::ZERO, &report).unwrap());
    assert!(e.is_installed(dir.path()));
    assert_eq!(std::fs::read(dir.path().join("test-model/model.bin")).unwrap(), data);
    assert_eq!(bolo_models::partial_bytes(dir.path(), &e), 0, "the partial file is gone");
    let first = events.lock().unwrap().iter().find(|e| e.phase == DownloadPhase::Downloading).unwrap().downloaded;
    assert!(first > partial, "picked up where it stopped");
    let ranges = server.ranges();
    assert_eq!(ranges.len(), 2);
    assert_eq!(ranges[0], None);
    let from: u64 =
        ranges[1].as_deref().unwrap().strip_prefix("bytes=").unwrap().strip_suffix('-').unwrap().parse().unwrap();
    assert!(from >= partial, "resumed with Range: bytes={from}-");
}

#[test]
fn a_cancel_before_the_start_downloads_nothing() {
    let data = body(MIB, 3);
    let server = Server::start(HashMap::from([("m.bin", data.clone())]), 64 * 1024, Duration::ZERO);
    let dir = tempfile::tempdir().unwrap();
    let e = entry("test-model", &server.url("m.bin"), &data);
    let cancel = CancelToken::default();
    cancel.cancel();
    let (events, report) = recorder();
    assert!(!run(&downloader(dir.path()), &e, None, &cancel, PROGRESS_INTERVAL, &report).unwrap());
    assert_eq!(phases(&events.lock().unwrap()), [DownloadPhase::Cancelled]);
}

#[test]
fn a_bad_checksum_fails_and_drops_the_bytes() {
    let data = body(2 * MIB, 4);
    let server = Server::start(HashMap::from([("m.bin", data.clone())]), 256 * 1024, Duration::ZERO);
    let dir = tempfile::tempdir().unwrap();
    let mut e = entry("test-model", &server.url("m.bin"), &data);
    e.download.as_mut().unwrap().sha256 = "0".repeat(64);
    let (events, report) = recorder();
    let err = run(&downloader(dir.path()), &e, None, &CancelToken::default(), PROGRESS_INTERVAL, &report).unwrap_err();
    assert!(err.to_string().contains("checksum mismatch"), "{err}");
    let events = events.lock().unwrap();
    let last = events.last().unwrap();
    assert_eq!(last.phase, DownloadPhase::Failed);
    assert!(last.error.as_deref().unwrap().contains("checksum mismatch"));
    assert!(!e.is_installed(dir.path()));
    assert_eq!(bolo_models::partial_bytes(dir.path(), &e), 0, "a corrupt download isn't resumed");
}

#[test]
fn the_vad_comes_along_and_done_is_sent_once_both_are_in() {
    let (model, vad) = (body(3 * MIB, 5), body(64 * 1024, 6));
    let server =
        Server::start(HashMap::from([("m.bin", model.clone()), ("vad.onnx", vad.clone())]), 256 * 1024, Duration::ZERO);
    let dir = tempfile::tempdir().unwrap();
    let (e, v) = (entry("test-model", &server.url("m.bin"), &model), entry("test-vad", &server.url("vad.onnx"), &vad));
    let (events, report) = recorder();
    assert!(run(&downloader(dir.path()), &e, Some(&v), &CancelToken::default(), PROGRESS_INTERVAL, &report).unwrap());
    assert!(e.is_installed(dir.path()) && v.is_installed(dir.path()));
    let events = events.lock().unwrap();
    assert!(events.iter().all(|d| d.id == "test-model"), "the VAD downloads silently");
    assert_eq!(events.iter().filter(|d| d.phase == DownloadPhase::Done).count(), 1);
    assert_eq!(events.last().unwrap().phase, DownloadPhase::Done);
}

// --- The `Bolo` calls ---

struct NoStt;

#[async_trait]
impl SttEngine for NoStt {
    fn id(&self) -> &str {
        "none"
    }
    async fn transcribe(&self, _audio: &[f32], _opts: &SttOptions) -> bolo_stt::Result<Transcript> {
        Ok(Transcript::default())
    }
}

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

struct Downloads(Mutex<Sender<ModelDownload>>);

impl BoloListener for Downloads {
    fn on_level(&self, _rms: f32) {}
    fn on_partial(&self, _text: String) {}
    fn on_final(&self, _d: Dictation) {}
    fn on_polished(&self, _d: Dictation) {}
    fn on_state(&self, _s: State) {}
    fn on_error(&self, _kind: ErrorKind, _message: String) {}
    fn on_settings_changed(&self, _settings: BoloSettings) {}
    fn on_notice(&self, _notice: bolo_ffi::Notice) {}
    fn on_download(&self, download: ModelDownload) {
        let _ = self.0.lock().unwrap().send(download);
    }
}

fn open(dir: &Path) -> (Arc<Bolo>, Receiver<ModelDownload>) {
    let s = |p: &str| dir.join(p).to_string_lossy().into_owned();
    let paths = BoloPaths { config_dir: s("files"), data_dir: s("files"), models_dir: s("no_backup/models") };
    let (tx, rx) = mpsc::channel();
    let bolo =
        Bolo::with_stt(paths, Arc::new(MemoryStore::default()), Arc::new(Downloads(Mutex::new(tx))), Arc::new(NoStt))
            .unwrap();
    (bolo, rx)
}

/// Puts model `id` in place as if it had been downloaded: its files and the marker.
fn fake_install(dir: &Path, id: &str) {
    let manifest = bolo_models::Manifest::bundled().unwrap();
    let e = manifest.get(id).unwrap();
    let root = dir.join("no_backup/models");
    let model = e.install_dir(&root);
    for f in e.files.values() {
        let p = model.join(f);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, b"x").unwrap();
    }
    std::fs::write(model.join(".bolo-installed"), b"x").unwrap();
}

fn ids(models: &[bolo_ffi::ModelInfo]) -> Vec<&str> {
    models.iter().map(|m| m.id.as_str()).collect()
}

#[test]
fn the_tier_and_the_recommendation_follow_the_reported_ram() {
    let dir = tempfile::tempdir().unwrap();
    let (bolo, _rx) = open(dir.path());
    // The 6 GB emulator reports 6 072 060 kB of MemTotal.
    assert_eq!(bolo.tier(6_072_060 / 1024), DeviceTier::High);
    assert_eq!(bolo.tier(5_632), DeviceTier::High);
    assert_eq!(bolo.tier(3_700), DeviceTier::Low);
    for language in [Language::Auto, Language::English, Language::Hinglish] {
        assert_eq!(bolo.recommend(5_929, language).unwrap().id, "nemotron-stream", "{language:?}");
    }
    assert_eq!(bolo.recommend(3_700, Language::English).unwrap().id, "moonshine-tiny");
    let auto = bolo.recommend(3_700, Language::Auto).unwrap();
    assert_eq!(auto.id, "whisper-base-hinglish", "Auto on a low-tier phone is the Hinglish model");
    assert!(auto.recommended && auto.hinglish && !auto.installed && !auto.downloading);
    assert_eq!(auto.license, "Apache-2.0");
    assert!(auto.attribution.contains("Oriserve"));
    // The archive, the unpacked model and a fifth, and the same for the VAD.
    let need = |archive: u64, unpacked: u64| archive + unpacked + unpacked / 5;
    assert_eq!(auto.space_needed, need(135_755_362, 160_592_756) + need(643_854, 643_854));
    assert_eq!(bolo.recommend(3_700, Language::Hinglish).unwrap().id, "whisper-base-hinglish");
    assert!(bolo.recommend(300, Language::Hinglish).is_none(), "nothing local fits");
}

#[test]
fn the_list_is_the_tiers_models_with_the_recommended_one_first() {
    let dir = tempfile::tempdir().unwrap();
    let (bolo, _rx) = open(dir.path());
    bolo.set_device(Some(3_700), Some(2));
    // Auto is the default language.
    assert_eq!(ids(&bolo.models()), ["whisper-base-hinglish", "moonshine-tiny"]);
    bolo.save_settings(BoloSettings { language: Language::English, ..bolo.settings() }).unwrap();
    let models = bolo.models();
    assert_eq!(ids(&models), ["moonshine-tiny", "whisper-base-hinglish"]);
    assert!(models[0].recommended && !models[1].recommended);
    bolo.set_device(Some(5_929), Some(4));
    assert_eq!(ids(&bolo.models()), ["nemotron-stream", "moonshine-tiny"]);
    fake_install(dir.path(), "moonshine-tiny");
    let models = bolo.models();
    assert!(models[1].installed && !models[0].installed);
}

#[test]
fn the_engine_runs_the_tiers_model_on_the_phones_budgets() {
    let dir = tempfile::tempdir().unwrap();
    let (bolo, _rx) = open(dir.path());
    bolo.save_settings(BoloSettings { engines: vec!["local:nemotron-stream".into()], ..bolo.settings() }).unwrap();
    bolo.set_device(Some(3_700), Some(2));
    let c = bolo.engine_config();
    assert_eq!(c.engines, ["local:whisper-base-hinglish"], "low tier, Auto: the Hinglish model, no dual pass");
    assert_eq!((c.idle_unload_secs, c.mic_warm_secs, c.threads), (20, 30, Some(2)));
    bolo.set_device(Some(5_929), Some(6));
    let c = bolo.engine_config();
    assert_eq!(c.engines, ["local:nemotron-stream"]);
    assert_eq!(c.threads, Some(4), "never more than four");
    assert_eq!(bolo.settings().engines, ["local:nemotron-stream"], "the settings keep the user's choice");
}

#[test]
fn every_bolo_on_the_models_hears_a_download() {
    let dir = tempfile::tempdir().unwrap();
    let (a, rx_a) = open(dir.path());
    let (_b, rx_b) = open(dir.path());
    fake_install(dir.path(), "moonshine-tiny");
    fake_install(dir.path(), "silero-vad");
    // Installed already, so nothing goes to the network.
    assert!(a.download_model("moonshine-tiny".into()).unwrap());
    for rx in [&rx_a, &rx_b] {
        let events: Vec<ModelDownload> = rx.try_iter().collect();
        assert_eq!(phases(&events), [DownloadPhase::Downloading, DownloadPhase::Done]);
        assert!(events.iter().all(|e| e.id == "moonshine-tiny"));
    }
}

#[test]
fn delete_removes_the_model_and_its_partial_download() {
    let dir = tempfile::tempdir().unwrap();
    let (bolo, _rx) = open(dir.path());
    bolo.set_device(Some(5_929), None);
    fake_install(dir.path(), "moonshine-tiny");
    let part = dir.path().join("no_backup/models/.downloads/moonshine-tiny.part");
    std::fs::create_dir_all(part.parent().unwrap()).unwrap();
    std::fs::write(&part, b"half").unwrap();
    bolo.delete_model("moonshine-tiny".into()).unwrap();
    assert!(!dir.path().join("no_backup/models/moonshine-tiny").exists());
    assert!(!part.exists());
    assert!(bolo.models().iter().all(|m| !m.installed));
    bolo.delete_model("moonshine-tiny".into()).unwrap();
    assert!(matches!(bolo.delete_model("nope".into()), Err(FfiError::Invalid { .. })));
    assert!(matches!(bolo.download_model("nope".into()), Err(FfiError::Invalid { .. })));
    assert!(!bolo.cancel_download("moonshine-tiny".into()), "nothing to cancel");
}

#[test]
fn space_messages_are_in_megabytes() {
    let e = FfiError::NoSpace { needed: 89_000_001, available: 12_000_000 };
    assert_eq!(e.to_string(), "the download needs 90 MB free, and there are 12 MB");
}
