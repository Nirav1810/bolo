//! The live Deepgram engine against a scripted local WebSocket server (no network).
//! Each test server plays Deepgram's side of one dictation.

#![cfg(feature = "cloud")]

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use bolo_stt::cloud::{DeepgramStreamEngine, http_client};
use bolo_stt::fallback::FallbackEngine;
use bolo_stt::{ApiKey, Language, SttEngine, SttError, SttOptions, SttStream, Transcript};
use futures::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;

fn results(text: &str, is_final: bool, from_finalize: bool) -> Message {
    Message::Text(
        serde_json::json!({
            "type": "Results", "channel_index": [0, 1], "start": 0.0, "duration": 1.0,
            "is_final": is_final, "speech_final": is_final, "from_finalize": from_finalize,
            "channel": {"alternatives": [{"transcript": text, "confidence": 0.9, "words": []}]},
            "metadata": {"request_id": "r", "model_info": {"name": "n", "version": "v", "arch": "a"}, "model_uuid": "u"}
        })
        .to_string()
        .into(),
    )
}

fn metadata() -> Message {
    Message::Text(
        r#"{"type":"Metadata","transaction_key":"deprecated","request_id":"r","sha256":"s","created":"c","duration":1.0,"channels":1}"#.into(),
    )
}

/// How the scripted server behaves after the upgrade.
#[derive(Clone, Copy)]
enum Script {
    /// Interim + final while audio arrives, a `from_finalize` result after `Finalize`,
    /// then `Metadata` and a normal close after `CloseStream`.
    Normal,
    /// Never answers `Finalize`; only `CloseStream` gets `Metadata` + close.
    NoFlush,
    /// Closes with 1011 `NET-0000` after the first audio frame.
    Drop,
}

#[derive(Default)]
struct Seen {
    path_and_query: String,
    authorization: String,
    audio_bytes: usize,
    texts: Vec<String>,
}

/// Starts a one-connection server and returns its `ws://` base URL.
async fn serve(script: Script, seen: Arc<Mutex<Seen>>, reject_status: Option<u16>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let seen_hs = seen.clone();
        #[allow(clippy::result_large_err)] // the signature is tungstenite's
        let callback = move |req: &Request, resp: Response| -> Result<Response, ErrorResponse> {
            let mut s = seen_hs.lock().unwrap();
            s.path_and_query = req.uri().path_and_query().map(|p| p.to_string()).unwrap_or_default();
            s.authorization =
                req.headers().get("Authorization").and_then(|v| v.to_str().ok()).unwrap_or_default().to_owned();
            if let Some(code) = reject_status {
                let mut e = ErrorResponse::new(Some(r#"{"err_code":"INVALID_AUTH"}"#.into()));
                *e.status_mut() = code.try_into().unwrap();
                return Err(e);
            }
            Ok(resp)
        };
        let Ok(mut ws) = tokio_tungstenite::accept_hdr_async(tcp, callback).await else { return };
        let mut frames = 0;
        while let Some(Ok(msg)) = ws.next().await {
            match msg {
                Message::Binary(b) => {
                    seen.lock().unwrap().audio_bytes += b.len();
                    frames += 1;
                    match (script, frames) {
                        (Script::Drop, 1) => {
                            let frame = CloseFrame { code: CloseCode::Error, reason: "NET-0000".into() };
                            let _ = ws.send(Message::Close(Some(frame))).await;
                            return;
                        }
                        (_, 1) => ws.send(results("hello", false, false)).await.unwrap(),
                        (_, 2) => ws.send(results("Hello world.", true, false)).await.unwrap(),
                        _ => {}
                    }
                }
                Message::Text(t) => {
                    seen.lock().unwrap().texts.push(t.to_string());
                    if t.contains("Finalize") && matches!(script, Script::Normal) {
                        ws.send(results("Kal milte hain.", true, true)).await.unwrap();
                    } else if t.contains("CloseStream") {
                        let _ = ws.send(metadata()).await;
                        let _ = ws.close(None).await;
                    }
                }
                _ => {}
            }
        }
    });
    format!("ws://{addr}/v1")
}

fn engine(ws_url: &str) -> DeepgramStreamEngine {
    // The HTTPS URL is only used by the batch fallback, which these tests never reach.
    DeepgramStreamEngine::with_urls(
        http_client().unwrap(),
        ApiKey::new("dg_secret"),
        "http://127.0.0.1:9",
        ws_url,
        "nova-3",
    )
    .unwrap()
}

/// 0.2 s of a quiet tone, fed in 10 ms chunks like the engine thread does.
fn speak(stream: &mut dyn SttStream) -> Vec<String> {
    let audio: Vec<f32> = (0..3200).map(|i| (i as f32 * 0.05).sin() * 0.1).collect();
    let mut partials = Vec::new();
    for chunk in audio.chunks(160) {
        stream.accept(chunk);
        if let Some(p) = stream.partial() {
            partials.push(p);
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    // Let the server's replies to the last frames arrive.
    let until = Instant::now() + Duration::from_millis(500);
    while Instant::now() < until && partials.last().map(String::as_str) != Some("Hello world.") {
        if let Some(p) = stream.partial() {
            partials.push(p);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    partials
}

fn opts() -> SttOptions {
    SttOptions { language: Language::Auto, dictionary: vec!["Bolo".into(), "Tauri app".into()], ..Default::default() }
}

#[tokio::test(flavor = "multi_thread")]
async fn streams_partials_then_final_after_finalize() {
    let seen = Arc::new(Mutex::new(Seen::default()));
    let url = serve(Script::Normal, seen.clone(), None).await;
    let e = engine(&url);
    assert!(e.supports_streaming());
    let t = tokio::task::spawn_blocking(move || {
        let mut s = e.stream(&opts()).expect("stream");
        let partials = speak(s.as_mut());
        assert_eq!(partials, ["hello", "Hello world."]);
        s.finish()
    })
    .await
    .unwrap()
    .unwrap();
    assert_eq!(t.text, "Hello world. Kal milte hain.");
    assert_eq!(t.engine_id, "deepgram-stream:nova-3");

    let s = seen.lock().unwrap();
    assert_eq!(s.authorization, "Token dg_secret");
    let q = &s.path_and_query;
    assert!(q.starts_with("/v1/listen?"), "{q}");
    for p in ["model=nova-3", "language=multi", "encoding=linear16", "sample_rate=16000", "interim_results=true"] {
        assert!(q.contains(p), "{p} missing from {q}");
    }
    assert_eq!(q.matches("keyterm=").count(), 2, "{q}");
    assert!(!q.contains("dg_secret"), "the key must not be in the URL");
    assert_eq!(s.audio_bytes, 3200 * 2, "every sample is sent as PCM16");
    assert_eq!(s.texts, [r#"{"type":"Finalize"}"#, r#"{"type":"CloseStream"}"#]);
}

#[tokio::test(flavor = "multi_thread")]
async fn final_arrives_without_a_finalize_flush() {
    let seen = Arc::new(Mutex::new(Seen::default()));
    let url = serve(Script::NoFlush, seen, None).await;
    let e = engine(&url);
    let t = tokio::task::spawn_blocking(move || {
        let mut s = e.stream(&opts()).unwrap();
        speak(s.as_mut());
        s.finish()
    })
    .await
    .unwrap()
    .unwrap();
    assert_eq!(t.text, "Hello world.");
}

#[tokio::test(flavor = "multi_thread")]
async fn rejected_key_is_an_auth_error() {
    let seen = Arc::new(Mutex::new(Seen::default()));
    let url = serve(Script::Normal, seen, Some(401)).await;
    let e = engine(&url);
    let r = tokio::task::spawn_blocking(move || {
        let mut s = e.stream(&opts()).unwrap();
        speak(s.as_mut());
        s.finish()
    })
    .await
    .unwrap();
    let err = r.unwrap_err();
    assert!(matches!(err, SttError::Auth), "{err:?}");
    assert!(!format!("{err} {err:?}").contains("dg_secret"));
}

#[tokio::test(flavor = "multi_thread")]
async fn dropped_connection_is_an_engine_error() {
    let seen = Arc::new(Mutex::new(Seen::default()));
    let url = serve(Script::Drop, seen, None).await;
    let e = engine(&url);
    let r = tokio::task::spawn_blocking(move || {
        let mut s = e.stream(&opts()).unwrap();
        speak(s.as_mut());
        s.finish()
    })
    .await
    .unwrap();
    let err = r.unwrap_err();
    assert!(matches!(&err, SttError::Network(m) if m.contains("1011") && m.contains("NET-0000")), "{err:?}");
}

/// Batch engine that records the audio it was given.
struct Recorder(Mutex<usize>);

#[async_trait]
impl SttEngine for Recorder {
    fn id(&self) -> &str {
        "recorder"
    }
    async fn transcribe(&self, audio: &[f32], _: &SttOptions) -> bolo_stt::Result<Transcript> {
        *self.0.lock().unwrap() = audio.len();
        Ok(Transcript { text: "from fallback".into(), engine_id: "recorder".into(), language: None })
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn fallback_chain_transcribes_the_audio_when_the_stream_fails() {
    let seen = Arc::new(Mutex::new(Seen::default()));
    let url = serve(Script::Drop, seen, None).await;
    let recorder = Arc::new(Recorder(Mutex::new(0)));
    let chain = FallbackEngine::new(vec![
        (Arc::new(engine(&url)) as Arc<dyn SttEngine>, Duration::from_secs(3)),
        (recorder.clone() as Arc<dyn SttEngine>, Duration::from_secs(3)),
    ])
    .unwrap();
    assert!(chain.supports_streaming());
    let t = tokio::task::spawn_blocking(move || {
        let mut s = chain.stream(&opts()).unwrap();
        speak(s.as_mut());
        s.finish()
    })
    .await
    .unwrap()
    .unwrap();
    assert_eq!(t.text, "from fallback");
    assert_eq!(*recorder.0.lock().unwrap(), 3200, "the fallback gets the whole dictation");
}
