//! The app controller: the engine's [`EngineEvents`], turned into [`AppEvent`]s for the
//! webviews.
//!
//! Dictation logic lives here, in Rust. The webviews render what they're sent and never
//! decide anything. Insertion (ticket 06) and history (ticket 07) hang off `on_final` and
//! `on_polished` here; for now the text goes to the log and the debug window.

use std::sync::Arc;

use bolo_core::{BoloError, DictationState, EngineEvents, FormattedOutput};
use serde::Serialize;

use crate::hotkey::Flags;

/// What the webviews are sent, as the payload of the [`crate::EVENT`] event. Mirrored by
/// `src/lib/events.ts`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum AppEvent {
    State { state: DictationState },
    Paused { paused: bool },
    Level { rms: f32 },
    Partial { text: String },
    Final { output: Output },
    Polished { output: Output },
    Error { message: String },
}

/// [`FormattedOutput`] as the webviews see it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Output {
    pub plain: String,
    pub markdown: String,
    pub html: Option<String>,
    pub timings: Timings,
    pub engine_id: String,
    pub language: bolo_core::LanguageMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Timings {
    pub stt_ms: f64,
    pub rules_ms: f64,
    pub llm_ms: f64,
    pub format_ms: f64,
    pub total_ms: f64,
}

impl From<FormattedOutput> for Output {
    fn from(o: FormattedOutput) -> Self {
        let t = o.timings;
        Self {
            plain: o.plain,
            markdown: o.markdown,
            html: o.html,
            timings: Timings {
                stt_ms: t.stt_ms,
                rules_ms: t.rules_ms,
                llm_ms: t.llm_ms,
                format_ms: t.format_ms,
                total_ms: t.total_ms,
            },
            engine_id: o.engine_id,
            language: o.language,
        }
    }
}

/// Where [`AppEvent`]s go: every webview in the app, or a recorder in tests.
pub trait Emit: Send + Sync {
    fn emit(&self, event: AppEvent);
}

/// The engine's event sink. Called on the engine's threads, so it only logs and posts.
pub struct Controller {
    emit: Arc<dyn Emit>,
    /// Tells the keyboard hook whether a dictation is listening (Esc cancels then).
    flags: Arc<Flags>,
}

impl Controller {
    pub fn new(emit: Arc<dyn Emit>, flags: Arc<Flags>) -> Self {
        Self { emit, flags }
    }
}

impl EngineEvents for Controller {
    fn on_level(&self, rms: f32) {
        self.emit.emit(AppEvent::Level { rms });
    }

    fn on_partial(&self, text: String) {
        self.emit.emit(AppEvent::Partial { text });
    }

    fn on_final(&self, out: FormattedOutput) {
        // Temporary (M3 ticket 01): the text goes to the log until insertion lands.
        tracing::info!(
            engine = %out.engine_id,
            language = ?out.language,
            total_ms = out.timings.total_ms,
            text = %out.plain,
            "final"
        );
        self.emit.emit(AppEvent::Final { output: out.into() });
    }

    fn on_polished(&self, out: FormattedOutput) {
        tracing::info!(llm_ms = out.timings.llm_ms, text = %out.plain, "polished");
        self.emit.emit(AppEvent::Polished { output: out.into() });
    }

    fn on_error(&self, err: BoloError) {
        tracing::warn!(%err, "engine error");
        self.emit.emit(AppEvent::Error { message: err.to_string() });
    }

    fn on_state(&self, state: DictationState) {
        tracing::debug!(?state, "state");
        self.flags.set_listening(state == DictationState::Listening);
        self.emit.emit(AppEvent::State { state });
    }
}

#[cfg(test)]
pub mod testing {
    use std::sync::Mutex;

    use super::{AppEvent, Emit};

    /// Records every event it's sent.
    #[derive(Debug, Default)]
    pub struct Recorder(pub Mutex<Vec<AppEvent>>);

    impl Recorder {
        pub fn take(&self) -> Vec<AppEvent> {
            std::mem::take(&mut *self.0.lock().unwrap())
        }
    }

    impl Emit for Recorder {
        fn emit(&self, event: AppEvent) {
            self.0.lock().unwrap().push(event);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bolo_core::{BoloError, DictationState, EngineEvents, FormattedOutput, LanguageMode, Timings};
    use serde_json::json;

    use super::testing::Recorder;
    use super::{AppEvent, Controller};
    use crate::hotkey::Flags;

    fn output(text: &str) -> FormattedOutput {
        FormattedOutput {
            plain: text.into(),
            markdown: text.into(),
            html: None,
            timings: Timings { total_ms: 120.0, ..Default::default() },
            engine_id: "local:nemotron-stream".into(),
            language: LanguageMode::English,
        }
    }

    #[test]
    fn engine_events_become_app_events_in_order() {
        let rec = Arc::new(Recorder::default());
        let flags = Arc::new(Flags::default());
        let c = Controller::new(rec.clone(), flags.clone());
        c.on_state(DictationState::Listening);
        assert!(flags.listening(), "the hook needs this for Esc");
        c.on_level(0.05);
        c.on_partial("hello".into());
        c.on_state(DictationState::Processing);
        assert!(!flags.listening());
        c.on_final(output("Hello."));
        c.on_polished(output("Hello!"));
        c.on_error(BoloError::Busy);
        let kinds: Vec<_> =
            rec.take().iter().map(|e| serde_json::to_value(e).unwrap()["kind"].as_str().unwrap().to_owned()).collect();
        assert_eq!(kinds, ["state", "level", "partial", "state", "final", "polished", "error"]);
    }

    #[test]
    fn payloads_match_the_webview_types() {
        let to_json = |e: AppEvent| serde_json::to_value(e).unwrap();
        assert_eq!(
            to_json(AppEvent::State { state: DictationState::Listening }),
            json!({"kind": "state", "state": "Listening"})
        );
        assert_eq!(to_json(AppEvent::Paused { paused: true }), json!({"kind": "paused", "paused": true}));
        assert_eq!(to_json(AppEvent::Partial { text: "hi".into() }), json!({"kind": "partial", "text": "hi"}));
        assert_eq!(to_json(AppEvent::Error { message: "boom".into() }), json!({"kind": "error", "message": "boom"}));
        let fin = to_json(AppEvent::Final { output: output("Hi.").into() });
        assert_eq!(fin["kind"], "final");
        assert_eq!(fin["output"]["plain"], "Hi.");
        assert_eq!(fin["output"]["language"], "english");
        assert_eq!(fin["output"]["engine_id"], "local:nemotron-stream");
        assert_eq!(fin["output"]["timings"]["total_ms"], 120.0);
        assert!(fin["output"]["html"].is_null());
    }
}
