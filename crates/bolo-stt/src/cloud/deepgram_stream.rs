//! Deepgram Nova-3 live streaming (`wss://api.deepgram.com/v1/listen`).
//!
//! Audio goes up as raw 16 kHz mono PCM16 while the user speaks. At release the stream
//! sends `Finalize` and then `CloseStream`, so the text arrives shortly after release
//! instead of after a whole-clip upload.
//!
//! Protocol (verified against Deepgram's docs on 2026-11-09):
//! - `Authorization: Token <key>` on the upgrade request. The query string is the same
//!   as the batch endpoint, plus `encoding=linear16&sample_rate=16000&channels=1&interim_results=true`.
//! - The server sends `Results` messages. An interim one (`is_final: false`) replaces
//!   the current interim text; a final one is appended. `from_finalize: true` marks the
//!   flush after our `Finalize`, but Deepgram doesn't guarantee one when little audio is
//!   pending. `CloseStream` always ends with `Metadata` and a close frame, so either of
//!   those also ends the dictation.
//! - `Error` messages are non-fatal. Fatal errors close the socket, for example with
//!   1008 `DATA-0000` or 1011 `NET-0001` when there was no audio for 10 s.
//! - Audio may be sent at no more than 1.25× real time, so `bolo replay --speed` must
//!   stay at 1 for this engine.
//!
//! The WebSocket runs on its own thread with a single-threaded tokio runtime. That keeps
//! [`SttStream::accept`] a cheap channel send from the engine thread.

use std::sync::Arc;
use std::sync::mpsc as std_mpsc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::{Error as WsError, Message};
use tokio_tungstenite::{Connector, connect_async_tls_with_config};

use super::deepgram::{self, DeepgramEngine, listen_params};
use crate::fallback::DEFAULT_CLOUD_TIMEOUT;
use crate::{ApiKey, Result, SttEngine, SttError, SttOptions, SttStream, Transcript};

pub const DEFAULT_WS_URL: &str = "wss://api.deepgram.com/v1";

const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
/// Release → final text budget. A connection that can't deliver by then is treated as
/// failed, so a fallback chain can take over.
const FINISH_TIMEOUT: Duration = DEFAULT_CLOUD_TIMEOUT;
/// Audio is sent in 40 ms frames (Deepgram suggests 20–100 ms).
const FRAME_SAMPLES: usize = 640;

const FINALIZE: &str = r#"{"type":"Finalize"}"#;
const CLOSE_STREAM: &str = r#"{"type":"CloseStream"}"#;

/// Streaming Deepgram engine. [`SttEngine::transcribe`] (used by `bolo run` and by
/// fallback chains) goes to the batch endpoint.
#[derive(Debug, Clone)]
pub struct DeepgramStreamEngine {
    batch: DeepgramEngine,
    key: ApiKey,
    ws_url: String,
    model: String,
    id: String,
    tls: Arc<rustls::ClientConfig>,
}

impl DeepgramStreamEngine {
    pub fn new(client: reqwest::Client, key: ApiKey) -> Result<Self> {
        Self::with_urls(client, key, deepgram::DEFAULT_BASE_URL, DEFAULT_WS_URL, deepgram::DEFAULT_MODEL)
    }

    /// `base_url` is the HTTPS API (batch fallback and prewarm), `ws_url` the live one.
    pub fn with_urls(client: reqwest::Client, key: ApiKey, base_url: &str, ws_url: &str, model: &str) -> Result<Self> {
        Ok(Self {
            batch: DeepgramEngine::with_model(client, key.clone(), base_url, model),
            key,
            ws_url: ws_url.trim_end_matches('/').to_owned(),
            model: model.to_owned(),
            id: format!("deepgram-stream:{model}"),
            tls: Arc::new(tls_config()?),
        })
    }

    fn url(&self, opts: &SttOptions) -> Result<String> {
        let mut q = listen_params(&self.model, opts);
        q.extend([
            ("encoding", "linear16".to_owned()),
            ("sample_rate", bolo_audio::SAMPLE_RATE.to_string()),
            ("channels", "1".to_owned()),
            ("interim_results", "true".to_owned()),
        ]);
        reqwest::Url::parse_with_params(&format!("{}/listen", self.ws_url), &q)
            .map(String::from)
            .map_err(|e| SttError::InvalidInput(e.to_string()))
    }
}

/// ring + the OS trust store, the same trust as the shared reqwest client.
fn tls_config() -> Result<rustls::ClientConfig> {
    use rustls_platform_verifier::BuilderVerifierExt;
    super::install_ring();
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .and_then(|b| b.with_platform_verifier())
        .map(|b| b.with_no_client_auth())
        .map_err(|e| SttError::Network(format!("tls setup: {e}")))
}

#[async_trait]
impl SttEngine for DeepgramStreamEngine {
    fn id(&self) -> &str {
        &self.id
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    async fn prewarm(&self) -> Result<()> {
        // Live connections are per dictation (Deepgram drops idle ones after 10 s); this
        // warms DNS and the HTTPS pool for the batch fallback.
        self.batch.prewarm().await
    }

    async fn transcribe(&self, audio: &[f32], opts: &SttOptions) -> Result<Transcript> {
        let mut t = self.batch.transcribe(audio, opts).await?;
        t.engine_id = self.id.clone();
        Ok(t)
    }

    fn stream(&self, opts: &SttOptions) -> Option<Box<dyn SttStream>> {
        let url = match self.url(opts) {
            Ok(u) => u,
            Err(e) => {
                tracing::warn!(error = %e, "deepgram stream url");
                return None;
            }
        };
        let auth = format!("Token {}", self.key.expose());
        let request = match url.into_client_request() {
            Ok(mut r) => match auth.parse() {
                Ok(v) => {
                    r.headers_mut().insert("Authorization", v);
                    r
                }
                Err(_) => return None,
            },
            Err(e) => {
                tracing::warn!(error = %e, "deepgram stream request");
                return None;
            }
        };
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let (ev_tx, ev_rx) = std_mpsc::channel();
        let connector = Connector::Rustls(self.tls.clone());
        let id = self.id.clone();
        let spawned = std::thread::Builder::new().name("bolo-deepgram".into()).spawn(move || {
            let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
                Ok(rt) => rt,
                Err(e) => {
                    let _ = ev_tx.send(Event::Done(Err(SttError::Network(e.to_string()))));
                    return;
                }
            };
            let result = rt.block_on(session(request, connector, cmd_rx, &ev_tx, id));
            let _ = ev_tx.send(Event::Done(result));
        });
        if let Err(e) = spawned {
            tracing::warn!(error = %e, "deepgram stream thread");
            return None;
        }
        Some(Box::new(DeepgramStream {
            cmds: cmd_tx,
            events: ev_rx,
            pending: Vec::with_capacity(FRAME_SAMPLES * 2),
            partial: None,
            done: None,
        }))
    }
}

enum Cmd {
    Audio(Vec<u8>),
    Finish,
}

enum Event {
    Partial(String),
    Done(Result<Transcript>),
}

struct DeepgramStream {
    cmds: mpsc::UnboundedSender<Cmd>,
    events: std_mpsc::Receiver<Event>,
    /// PCM16 LE bytes not yet sent (less than one frame).
    pending: Vec<u8>,
    partial: Option<String>,
    /// The session ended before `finish` (an error, usually).
    done: Option<Result<Transcript>>,
}

impl DeepgramStream {
    fn drain_events(&mut self) {
        while let Ok(ev) = self.events.try_recv() {
            match ev {
                Event::Partial(p) => self.partial = Some(p),
                Event::Done(r) => self.done = Some(r),
            }
        }
    }

    fn send_pending(&mut self) {
        if !self.pending.is_empty() {
            // Err: the session already ended; `finish` reports why.
            let _ = self.cmds.send(Cmd::Audio(std::mem::take(&mut self.pending)));
        }
    }
}

impl SttStream for DeepgramStream {
    fn accept(&mut self, samples: &[f32]) {
        for &s in samples {
            let v = (s.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16;
            self.pending.extend_from_slice(&v.to_le_bytes());
        }
        if self.pending.len() >= FRAME_SAMPLES * 2 {
            self.send_pending();
        }
    }

    fn partial(&mut self) -> Option<String> {
        self.drain_events();
        self.partial.take()
    }

    fn finish(mut self: Box<Self>) -> Result<Transcript> {
        self.send_pending();
        let _ = self.cmds.send(Cmd::Finish);
        self.drain_events();
        if let Some(r) = self.done.take() {
            return r;
        }
        let deadline = Instant::now() + FINISH_TIMEOUT;
        loop {
            match self.events.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                Ok(Event::Partial(_)) => {}
                Ok(Event::Done(r)) => return r,
                Err(std_mpsc::RecvTimeoutError::Timeout) => return Err(SttError::Timeout(FINISH_TIMEOUT)),
                Err(std_mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(SttError::Network("deepgram stream thread ended".into()));
                }
            }
        }
    }
}

/// One live connection: connect, pump audio up and results down, until the final text.
/// Dropping the stream (a cancelled dictation) closes the connection.
async fn session(
    request: tokio_tungstenite::tungstenite::handshake::client::Request,
    connector: Connector,
    mut cmds: mpsc::UnboundedReceiver<Cmd>,
    events: &std_mpsc::Sender<Event>,
    engine_id: String,
) -> Result<Transcript> {
    let (ws, _) =
        tokio::time::timeout(CONNECT_TIMEOUT, connect_async_tls_with_config(request, None, true, Some(connector)))
            .await
            .map_err(|_| SttError::Timeout(CONNECT_TIMEOUT))?
            .map_err(map_ws_error)?;
    let (mut sink, mut incoming) = ws.split();
    let mut results = Assembler::default();
    // Set once `Finalize` + `CloseStream` are sent; bounds the wait for the server.
    let mut finish_deadline: Option<tokio::time::Instant> = None;
    loop {
        let finishing = finish_deadline.is_some();
        tokio::select! {
            cmd = cmds.recv(), if !finishing => match cmd {
                Some(Cmd::Audio(bytes)) => {
                    sink.send(Message::Binary(bytes.into())).await.map_err(map_ws_error)?;
                }
                Some(Cmd::Finish) => {
                    sink.send(Message::Text(FINALIZE.into())).await.map_err(map_ws_error)?;
                    sink.send(Message::Text(CLOSE_STREAM.into())).await.map_err(map_ws_error)?;
                    finish_deadline = Some(tokio::time::Instant::now() + FINISH_TIMEOUT);
                }
                None => {
                    // Cancelled: stop billing promptly.
                    let _ = sink.send(Message::Text(CLOSE_STREAM.into())).await;
                    let _ = sink.close().await;
                    return Err(SttError::InvalidInput("dictation cancelled".into()));
                }
            },
            msg = incoming.next() => match msg {
                Some(Ok(Message::Text(text))) => match results.handle(text.as_str()) {
                    Step::Partial(p) => {
                        let _ = events.send(Event::Partial(p));
                    }
                    Step::Flushed | Step::Metadata if finishing => return Ok(results.transcript(&engine_id)),
                    Step::Flushed | Step::Metadata | Step::None => {}
                },
                Some(Ok(Message::Close(frame))) => {
                    return match frame {
                        Some(f) if f.code != CloseCode::Normal => Err(close_error(&f)),
                        _ if finishing => Ok(results.transcript(&engine_id)),
                        _ => Err(SttError::Network("deepgram closed the stream".into())),
                    };
                }
                Some(Ok(_)) => {}
                Some(Err(WsError::ConnectionClosed)) | None if finishing => {
                    return Ok(results.transcript(&engine_id));
                }
                Some(Err(e)) => return Err(map_ws_error(e)),
                None => return Err(SttError::Network("deepgram closed the stream".into())),
            },
            _ = sleep_until(finish_deadline) => return Err(SttError::Timeout(FINISH_TIMEOUT)),
        }
    }
}

async fn sleep_until(deadline: Option<tokio::time::Instant>) {
    match deadline {
        Some(d) => tokio::time::sleep_until(d).await,
        None => std::future::pending().await,
    }
}

fn close_error(f: &CloseFrame) -> SttError {
    let reason: String = f.reason.as_str().chars().take(200).collect();
    SttError::Network(format!("deepgram closed the stream ({}): {reason}", u16::from(f.code)))
}

/// Handshake and transport errors. The request (which carries the key) is never formatted.
fn map_ws_error(e: WsError) -> SttError {
    match e {
        WsError::Http(resp) => {
            let status = resp.status().as_u16();
            if status == 401 || status == 403 {
                return SttError::Auth;
            }
            let body = resp.body().as_deref().map(String::from_utf8_lossy).unwrap_or_default();
            SttError::Status { status, body: body.chars().take(300).collect() }
        }
        WsError::Io(e) => SttError::Network(e.to_string()),
        WsError::ConnectionClosed | WsError::AlreadyClosed => SttError::Network("deepgram closed the stream".into()),
        other => SttError::Network(other.to_string()),
    }
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum ServerMsg {
    Results {
        channel: Channel,
        #[serde(default)]
        is_final: bool,
        #[serde(default)]
        from_finalize: bool,
    },
    Metadata {},
    Error {
        #[serde(default)]
        description: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
struct Channel {
    alternatives: Vec<Alternative>,
}

#[derive(Deserialize)]
struct Alternative {
    transcript: String,
    /// Only with `language=multi`, most-used first.
    #[serde(default)]
    languages: Vec<String>,
}

/// What a server message meant for the dictation.
#[derive(Debug, PartialEq, Eq)]
enum Step {
    /// The running text changed.
    Partial(String),
    /// The flush after `Finalize`.
    Flushed,
    /// The summary sent after `CloseStream`, just before the server closes.
    Metadata,
    None,
}

/// Builds the dictation's text from `Results` messages: final segments in order, then
/// the current interim one.
#[derive(Default)]
struct Assembler {
    finals: Vec<String>,
    interim: String,
    language: Option<String>,
    shown: String,
}

impl Assembler {
    fn handle(&mut self, msg: &str) -> Step {
        let msg = match serde_json::from_str::<ServerMsg>(msg) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(error = %e, "unreadable deepgram message");
                return Step::None;
            }
        };
        match msg {
            ServerMsg::Results { channel, is_final, from_finalize } => {
                let alt = channel.alternatives.into_iter().next();
                let text = alt.as_ref().map(|a| a.transcript.trim().to_owned()).unwrap_or_default();
                if is_final {
                    if !text.is_empty() {
                        self.finals.push(text);
                        if let Some(l) = alt.and_then(|a| a.languages.into_iter().next()) {
                            self.language = Some(l);
                        }
                    }
                    self.interim.clear();
                } else {
                    self.interim = text;
                }
                if from_finalize {
                    return Step::Flushed;
                }
                let running = self.running_text();
                if running == self.shown {
                    return Step::None;
                }
                self.shown = running.clone();
                Step::Partial(running)
            }
            ServerMsg::Metadata {} => Step::Metadata,
            ServerMsg::Error { description } => {
                tracing::warn!(%description, "deepgram stream error message");
                Step::None
            }
            ServerMsg::Other => Step::None,
        }
    }

    fn running_text(&self) -> String {
        let mut parts: Vec<&str> = self.finals.iter().map(String::as_str).collect();
        if !self.interim.is_empty() {
            parts.push(&self.interim);
        }
        parts.join(" ")
    }

    /// The final transcript. Interim text left unconfirmed is dropped.
    fn transcript(&self, engine_id: &str) -> Transcript {
        Transcript { text: self.finals.join(" "), engine_id: engine_id.to_owned(), language: self.language.clone() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn results(text: &str, is_final: bool, from_finalize: bool) -> String {
        serde_json::json!({
            "type": "Results", "channel_index": [0, 1], "start": 0.0, "duration": 1.0,
            "is_final": is_final, "speech_final": is_final, "from_finalize": from_finalize,
            "channel": {"alternatives": [{"transcript": text, "confidence": 0.9, "words": [], "languages": ["hi"]}]},
            "metadata": {"request_id": "r", "model_info": {"name": "n", "version": "v", "arch": "a"}, "model_uuid": "u"}
        })
        .to_string()
    }

    #[test]
    fn interim_replaces_and_final_appends() {
        let mut a = Assembler::default();
        assert_eq!(a.handle(&results("hello", false, false)), Step::Partial("hello".into()));
        assert_eq!(a.handle(&results("hello world", false, false)), Step::Partial("hello world".into()));
        assert_eq!(a.handle(&results("Hello world.", true, false)), Step::Partial("Hello world.".into()));
        // An empty interim after a final changes nothing visible.
        assert_eq!(a.handle(&results("", false, false)), Step::None);
        assert_eq!(a.handle(&results("kal", false, false)), Step::Partial("Hello world. kal".into()));
        assert_eq!(a.handle(&results("Kal milte hain.", true, true)), Step::Flushed);
        let t = a.transcript("deepgram-stream:nova-3");
        assert_eq!(t.text, "Hello world. Kal milte hain.");
        assert_eq!(t.language.as_deref(), Some("hi"));
    }

    #[test]
    fn unconfirmed_interim_is_not_final() {
        let mut a = Assembler::default();
        a.handle(&results("Send it.", true, false));
        a.handle(&results("and then", false, false));
        assert_eq!(a.transcript("x").text, "Send it.");
    }

    #[test]
    fn other_messages() {
        let mut a = Assembler::default();
        let meta = r#"{"type":"Metadata","transaction_key":"deprecated","request_id":"r","sha256":"s","created":"c","duration":1.0,"channels":1}"#;
        assert_eq!(a.handle(meta), Step::Metadata);
        assert_eq!(a.handle(r#"{"type":"SpeechStarted","channel":[0],"timestamp":0.5}"#), Step::None);
        assert_eq!(a.handle(r#"{"type":"UtteranceEnd","channel":[0],"last_word_end":1.2}"#), Step::None);
        assert_eq!(
            a.handle(r#"{"type":"Error","variant":"SchemaError","description":"bad","message":"x"}"#),
            Step::None
        );
        assert_eq!(a.handle("not json"), Step::None);
    }

    #[test]
    fn url_has_live_params_and_keyterms() {
        let Ok(client) = crate::cloud::http_client() else { return };
        let e = DeepgramStreamEngine::new(client, ApiKey::new("k")).unwrap();
        let o = SttOptions { dictionary: vec!["Bolo".into(), "Tauri app".into()], ..Default::default() };
        let url = e.url(&o).unwrap();
        assert!(url.starts_with("wss://api.deepgram.com/v1/listen?model=nova-3&language=multi"), "{url}");
        for p in ["encoding=linear16", "sample_rate=16000", "channels=1", "interim_results=true"] {
            assert!(url.contains(p), "{url}");
        }
        assert_eq!(url.matches("keyterm=").count(), 2);
        assert_eq!(e.id(), "deepgram-stream:nova-3");
    }
}
