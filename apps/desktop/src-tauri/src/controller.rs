//! The app controller: the engine's [`EngineEvents`], turned into [`AppEvent`]s for the
//! webviews, and the instant and polished text into the app in front.
//!
//! Dictation logic lives here, in Rust. The webviews render what they're sent and never
//! decide anything. `on_final` pastes the instant text and `on_polished` swaps in the
//! polished text through the [`Inserter`]; the [`Journal`] records both in history.

use std::sync::Arc;
use std::time::Instant;

use bolo_core::{BoloError, DictationState, EngineEvents, FormattedOutput};
use serde::Serialize;

use crate::hotkey::Flags;
use crate::insert::{Inserted, Inserter, Swapped};
use crate::journal::Journal;
use crate::pill::View;

/// What the webviews are sent, as the payload of the [`crate::EVENT`] event. Mirrored by
/// `src/lib/events.ts`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum AppEvent {
    State {
        state: DictationState,
    },
    Paused {
        paused: bool,
    },
    Level {
        rms: f32,
    },
    Partial {
        text: String,
    },
    Final {
        output: Output,
    },
    Polished {
        output: Output,
        /// Whether it replaced the instant text in the app (ADR-0018).
        swapped: bool,
    },
    /// Something the user should know that isn't an error, for the pill. `hint` is the
    /// thing to do about it, shown dimmed after the message.
    Notice {
        message: String,
        hint: Option<String>,
    },
    Error {
        message: String,
    },
    /// What the overlay pill shows (sent by [`crate::overlay`], to the pill webview).
    Pill {
        view: View,
    },
    /// The self-updater's state (ADR-0019), for the settings window.
    Update {
        status: crate::updates::Status,
    },
}

/// The notice when the window in front runs elevated and won't take Bolo's paste: the
/// text is left on the clipboard for the user to paste.
pub const BLOCKED_NOTICE: &str = "Copied. Couldn't paste into an admin window";
pub const BLOCKED_HINT: &str = "Ctrl+V";

impl AppEvent {
    /// The notice for a paste the window in front blocked.
    pub fn blocked() -> Self {
        Self::Notice { message: BLOCKED_NOTICE.into(), hint: Some(BLOCKED_HINT.into()) }
    }
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

/// The engine's event sink. Called on the engine's threads.
pub struct Controller {
    emit: Arc<dyn Emit>,
    /// Tells the keyboard hook whether a dictation is listening (Esc cancels then).
    flags: Arc<Flags>,
    inserter: Arc<Inserter>,
    journal: Arc<Journal>,
}

impl Controller {
    pub fn new(emit: Arc<dyn Emit>, flags: Arc<Flags>, inserter: Arc<Inserter>, journal: Arc<Journal>) -> Self {
        Self { emit, flags, inserter, journal }
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
        let inserted = self.inserter.insert(&out, Instant::now());
        tracing::info!(engine = %out.engine_id, language = ?out.language, ?inserted, "final");
        match inserted {
            Inserted::Pasted | Inserted::Empty => {}
            Inserted::Blocked => self.emit.emit(AppEvent::blocked()),
            Inserted::Failed(e) => self.emit.emit(AppEvent::Error { message: format!("could not paste: {e}") }),
        }
        // After the paste, so history never delays the text.
        self.journal.finished(&out);
        self.emit.emit(AppEvent::Final { output: out.into() });
    }

    fn on_polished(&self, out: FormattedOutput) {
        let llm_ms = out.timings.llm_ms;
        let swapped = self.inserter.polished(&out, Instant::now());
        let swap = match &swapped {
            Swapped::Swapped => {
                tracing::info!(llm_ms, "polish swapped in");
                "swapped"
            }
            Swapped::Skipped(why) => {
                tracing::info!(llm_ms, skip = why.as_str(), "polish not swapped");
                why.as_str()
            }
            Swapped::Failed(e) => {
                tracing::warn!(llm_ms, %e, "the polish swap failed");
                "failed"
            }
        };
        self.journal.polished(&out, swap);
        self.emit.emit(AppEvent::Polished { output: out.into(), swapped: swapped == Swapped::Swapped });
    }

    fn on_error(&self, err: BoloError) {
        tracing::warn!(%err, "engine error");
        self.emit.emit(AppEvent::Error { message: err.to_string() });
    }

    fn on_state(&self, state: DictationState) {
        tracing::debug!(?state, "state");
        self.flags.set_listening(state == DictationState::Listening);
        if state == DictationState::Listening {
            // A new dictation: the previous one's polish no longer replaces anything.
            self.inserter.drop_pending();
        }
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
    use crate::desktop::Key;
    use crate::desktop::fake::{FakeDesktop, Op};
    use crate::hotkey::Flags;
    use crate::insert::testing::FakeTimer;
    use crate::insert::{Inserter, RESTORE_AFTER, Target};
    use crate::journal::Journal;
    use bolo_core::OutputFormat;
    use bolo_store::History;

    struct Rig {
        rec: Arc<Recorder>,
        flags: Arc<Flags>,
        desktop: Arc<FakeDesktop>,
        timer: Arc<FakeTimer>,
        inserter: Arc<Inserter>,
        journal: Arc<Journal>,
        c: Controller,
    }

    fn rig() -> Rig {
        let rec = Arc::new(Recorder::default());
        let flags = Arc::new(Flags::default());
        let desktop = Arc::new(FakeDesktop::in_front("notepad.exe", "Untitled - Notepad"));
        let timer = Arc::new(FakeTimer::default());
        let inserter = Inserter::new(desktop.clone(), timer.clone(), flags.clone(), || {});
        let journal = Arc::new(Journal::new(Some(History::open_in_memory().unwrap())));
        let c = Controller::new(rec.clone(), flags.clone(), inserter.clone(), journal.clone());
        Rig { rec, flags, desktop, timer, inserter, journal, c }
    }

    fn kinds(rec: &Recorder) -> Vec<String> {
        rec.take().iter().map(|e| serde_json::to_value(e).unwrap()["kind"].as_str().unwrap().to_owned()).collect()
    }

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
        let Rig { rec, flags, c, .. } = rig();
        c.on_state(DictationState::Listening);
        assert!(flags.listening(), "the hook needs this for Esc");
        c.on_level(0.05);
        c.on_partial("hello".into());
        c.on_state(DictationState::Processing);
        assert!(!flags.listening());
        c.on_final(output("Hello."));
        c.on_polished(output("Hello!"));
        c.on_error(BoloError::Busy);
        assert_eq!(kinds(&rec), ["state", "level", "partial", "state", "final", "polished", "error"]);
    }

    #[test]
    fn the_final_text_is_pasted_polish_swapped_and_the_clipboard_restored() {
        let r = rig();
        r.desktop.user_copies("mine");
        r.inserter.begin(Target::default(), true);
        r.c.on_state(DictationState::Listening);
        r.c.on_state(DictationState::Processing);
        r.c.on_final(output("helo"));
        let ops = r.desktop.take_ops();
        assert_eq!(ops.last(), Some(&Op::Keys(vec![Key::Paste])), "{ops:?}");
        r.c.on_polished(output("Hello."));
        let ops = r.desktop.take_ops();
        assert_eq!(ops.last(), Some(&Op::Keys([[Key::ShiftLeft; 4].as_slice(), &[Key::Paste]].concat())));
        r.timer.fire(RESTORE_AFTER);
        assert_eq!(r.desktop.text().as_deref(), Some("mine"));
    }

    fn history(j: &Journal) -> Vec<(String, Option<String>, Option<String>)> {
        let rows = j.with_history(|h| h.latest(10).unwrap()).unwrap();
        rows.into_iter().map(|r| (r.dictation.final_text, r.dictation.polished_text, r.dictation.swap)).collect()
    }

    #[test]
    fn each_dictation_is_recorded_with_its_polish_and_swap_outcome() {
        let r = rig();
        r.journal.begin("notepad.exe", OutputFormat::Plain);
        r.inserter.begin(Target::default(), true);
        r.c.on_final(output("helo"));
        r.c.on_polished(output("Hello."));
        r.journal.begin("notepad.exe", OutputFormat::Plain);
        r.inserter.begin(Target::default(), true);
        r.c.on_final(output(
            "one
two",
        ));
        r.c.on_polished(output(
            "One.
Two.",
        ));
        assert_eq!(
            history(&r.journal),
            [
                (
                    "one
two"
                    .into(),
                    Some(
                        "One.
Two."
                            .into()
                    ),
                    Some("multiline".into())
                ),
                ("helo".into(), Some("Hello.".into()), Some("swapped".into())),
            ]
        );
    }

    #[test]
    fn a_blocked_paste_is_still_recorded() {
        let r = rig();
        r.desktop.set_blocked(true);
        r.journal.begin("notepad.exe", OutputFormat::Plain);
        r.c.on_final(output("Hello."));
        assert_eq!(history(&r.journal), [("Hello.".into(), None, None)]);
    }

    #[test]
    fn a_new_dictation_supersedes_the_pending_swap() {
        let r = rig();
        r.inserter.begin(Target::default(), true);
        r.c.on_final(output("helo"));
        r.c.on_state(DictationState::Listening);
        r.desktop.take_ops();
        r.c.on_polished(output("Hello."));
        assert_eq!(r.desktop.take_ops(), [], "the old polish lands nowhere");
        assert_eq!(kinds(&r.rec), ["final", "state", "polished"], "but the webviews still see it");
    }

    #[test]
    fn the_polished_event_says_whether_it_was_swapped_in() {
        let swapped = |r: &Rig| {
            r.rec.take().into_iter().find_map(|e| match e {
                AppEvent::Polished { swapped, .. } => Some(swapped),
                _ => None,
            })
        };
        let r = rig();
        r.inserter.begin(Target::default(), true);
        r.c.on_final(output("helo"));
        r.c.on_polished(output("Hello."));
        assert_eq!(swapped(&r), Some(true));
        r.inserter.begin(Target::default(), true);
        r.c.on_final(output("helo"));
        r.desktop.focus("chrome.exe", "Inbox", 7);
        r.c.on_polished(output("Hello."));
        assert_eq!(swapped(&r), Some(false), "the user moved to another window");
    }

    #[test]
    fn a_blocked_paste_sends_a_notice() {
        let r = rig();
        r.desktop.set_blocked(true);
        r.c.on_final(output("Hello."));
        let events = r.rec.take();
        assert_eq!(events[0], AppEvent::blocked());
        assert!(matches!(events[1], AppEvent::Final { .. }));
        assert_eq!(r.desktop.text().as_deref(), Some("Hello."));
        assert!(r.timer.delays().is_empty());
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
        assert_eq!(
            to_json(AppEvent::Notice { message: "hm".into(), hint: None }),
            json!({"kind": "notice", "message": "hm", "hint": null})
        );
        assert_eq!(
            to_json(AppEvent::Pill { view: crate::pill::View::Polishing }),
            json!({"kind": "pill", "view": {"state": "polishing"}})
        );
        let fin = to_json(AppEvent::Final { output: output("Hi.").into() });
        assert_eq!(fin["kind"], "final");
        assert_eq!(fin["output"]["plain"], "Hi.");
        assert_eq!(fin["output"]["language"], "english");
        assert_eq!(fin["output"]["engine_id"], "local:nemotron-stream");
        assert_eq!(fin["output"]["timings"]["total_ms"], 120.0);
        assert!(fin["output"]["html"].is_null());
    }
}
