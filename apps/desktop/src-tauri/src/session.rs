//! Starting, stopping and pausing dictations: what the tray and the push-to-talk key ask
//! for, decided here so it's testable without a desktop.

use bolo_core::{BoloError, DictationState, Engine, Polish, TargetContext};

use crate::insert::Target;
use crate::rules::Plan;

/// The engine methods the session drives. [`Engine`] is the real one.
pub trait EngineControl {
    fn start(&self, ctx: TargetContext) -> Result<(), BoloError>;
    /// Polish from the next dictation on; the model stays loaded.
    fn set_polish(&self, polish: Polish);
    fn stop(&self) -> Result<(), BoloError>;
    fn cancel(&self);
    /// Unload the model now.
    fn release(&self);
    fn state(&self) -> DictationState;
}

impl EngineControl for Engine {
    fn start(&self, ctx: TargetContext) -> Result<(), BoloError> {
        self.start_dictation(ctx)
    }
    fn set_polish(&self, polish: Polish) {
        Engine::set_polish(self, polish);
    }
    fn stop(&self) -> Result<(), BoloError> {
        self.stop_dictation()
    }
    fn cancel(&self) {
        self.cancel_dictation();
    }
    fn release(&self) {
        self.release_resources();
    }
    fn state(&self) -> DictationState {
        Engine::state(self)
    }
}

/// What a toggle did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Toggled {
    /// A dictation started into `app`. Its text goes in as `target` says, and polish
    /// follows when `polish` is set.
    Started {
        app: String,
        target: Target,
        polish: bool,
    },
    Stopped,
    /// Bolo is paused, so nothing happened.
    Paused,
    /// Push-to-talk was held while a dictation was already listening; it carries on.
    AlreadyListening,
}

#[derive(Debug, Default)]
pub struct Session {
    paused: bool,
    /// The polish config last sent to the engine; `None` before the first dictation.
    polish: Option<Polish>,
}

impl Session {
    pub fn paused(&self) -> bool {
        self.paused
    }

    /// Stop the dictation that is listening, or start one with the [`Plan`] for the
    /// window in front (`plan` is called only to start).
    pub fn toggle(&mut self, engine: &dyn EngineControl, plan: impl FnOnce() -> Plan) -> Result<Toggled, BoloError> {
        if self.paused {
            return Ok(Toggled::Paused);
        }
        if engine.state() == DictationState::Listening {
            engine.stop()?;
            return Ok(Toggled::Stopped);
        }
        self.start(engine, plan())
    }

    /// The push-to-talk key was held: start a dictation for the window in front. One
    /// already listening (started from the tray) carries on, and the key's release stops it.
    ///
    /// It doesn't look at [`EngineControl::state`] first: a stop sent just before is still
    /// queued, and the engine runs its commands in order, so a start after it succeeds.
    pub fn press(&mut self, engine: &dyn EngineControl, plan: impl FnOnce() -> Plan) -> Result<Toggled, BoloError> {
        if self.paused {
            return Ok(Toggled::Paused);
        }
        match self.start(engine, plan()) {
            Err(BoloError::Busy) => Ok(Toggled::AlreadyListening),
            other => other,
        }
    }

    /// Polish goes to the engine only when the plan's differs from the last one sent, and
    /// before the start, which the engine runs after it.
    fn start(&mut self, engine: &dyn EngineControl, plan: Plan) -> Result<Toggled, BoloError> {
        let polish = plan.polish != Polish::Off;
        if self.polish.as_ref() != Some(&plan.polish) {
            engine.set_polish(plan.polish.clone());
            self.polish = Some(plan.polish);
        }
        let app = plan.ctx.app_id.clone();
        engine.start(plan.ctx)?;
        Ok(Toggled::Started { app, target: plan.target, polish })
    }

    /// The push-to-talk key was released: stop the dictation if one is listening.
    /// Whether one was.
    pub fn release(&self, engine: &dyn EngineControl) -> Result<bool, BoloError> {
        if engine.state() != DictationState::Listening {
            return Ok(false);
        }
        engine.stop()?;
        Ok(true)
    }

    /// Pausing drops a dictation in progress and unloads the model; nothing starts
    /// until Bolo is resumed.
    pub fn set_paused(&mut self, paused: bool, engine: Option<&dyn EngineControl>) {
        self.paused = paused;
        if paused && let Some(engine) = engine {
            engine.cancel();
            engine.release();
        }
    }

    /// A new engine was built with its own polish config: send the next plan's.
    pub fn engine_replaced(&mut self) {
        self.polish = None;
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use bolo_core::{BoloError, DictationState, OutputFormat, Polish, TargetContext};

    use super::{EngineControl, Session, Toggled};
    use crate::insert::{Chord, Target};
    use crate::rules::{AppRules, Plan, PolishPrefs, plan};

    #[derive(Debug, PartialEq)]
    enum Call {
        Start(String, Option<String>),
        /// The polish app, or `None` for off.
        SetPolish(Option<String>),
        Stop,
        Cancel,
        Release,
    }

    struct FakeEngine {
        state: RefCell<DictationState>,
        calls: RefCell<Vec<Call>>,
        fail_start: bool,
    }

    impl FakeEngine {
        fn new() -> Self {
            Self { state: RefCell::new(DictationState::Idle), calls: RefCell::default(), fail_start: false }
        }

        fn polish_calls(&self) -> Vec<Option<String>> {
            self.calls
                .borrow()
                .iter()
                .filter_map(|c| match c {
                    Call::SetPolish(app) => Some(app.clone()),
                    _ => None,
                })
                .collect()
        }
    }

    impl EngineControl for FakeEngine {
        fn start(&self, ctx: TargetContext) -> Result<(), BoloError> {
            if self.fail_start {
                return Err(BoloError::ModelMissing("nemotron-stream".into()));
            }
            if *self.state.borrow() == DictationState::Listening {
                return Err(BoloError::Busy);
            }
            self.calls.borrow_mut().push(Call::Start(ctx.app_id, ctx.window_title));
            *self.state.borrow_mut() = DictationState::Listening;
            Ok(())
        }
        fn set_polish(&self, polish: Polish) {
            let app = match polish {
                Polish::On { app, .. } => Some(app),
                Polish::Off => None,
            };
            self.calls.borrow_mut().push(Call::SetPolish(app));
        }
        fn stop(&self) -> Result<(), BoloError> {
            self.calls.borrow_mut().push(Call::Stop);
            *self.state.borrow_mut() = DictationState::Processing;
            Ok(())
        }
        fn cancel(&self) {
            self.calls.borrow_mut().push(Call::Cancel);
            *self.state.borrow_mut() = DictationState::Idle;
        }
        fn release(&self) {
            self.calls.borrow_mut().push(Call::Release);
        }
        fn state(&self) -> DictationState {
            *self.state.borrow()
        }
    }

    /// The bundled rules' plan for a window, with polish on and the key stored.
    fn window(exe: &str, title: &str) -> impl FnOnce() -> Plan + use<> {
        let prefs = PolishPrefs { enabled: true, has_key: true, ..Default::default() };
        let p = plan(exe, title, &AppRules::bundled(), &prefs);
        move || p
    }

    fn never() -> Plan {
        panic!("the plan is only needed to start")
    }

    #[test]
    fn toggle_starts_with_the_plan_for_the_window_in_front_then_stops() {
        let (engine, mut session) = (FakeEngine::new(), Session::default());
        let started = session.toggle(&engine, window("chrome.exe", "Inbox - Gmail")).unwrap();
        let html = Target { format: OutputFormat::Html, chord: Chord::CtrlV, multiline_swap_safe: false };
        assert_eq!(started, Toggled::Started { app: "chrome.exe".into(), target: html, polish: true });
        assert_eq!(session.toggle(&engine, never).unwrap(), Toggled::Stopped);
        assert_eq!(
            *engine.calls.borrow(),
            [
                Call::SetPolish(Some("Gmail".into())),
                Call::Start("chrome.exe".into(), Some("Inbox - Gmail".into())),
                Call::Stop
            ]
        );
    }

    #[test]
    fn polish_is_sent_only_when_it_changes() {
        let (engine, mut session) = (FakeEngine::new(), Session::default());
        for exe in ["notepad.exe", "notepad.exe", "code.exe", "cmd.exe", "slack.exe"] {
            session.press(&engine, window(exe, "")).unwrap();
            session.release(&engine).unwrap();
        }
        assert_eq!(engine.polish_calls(), [Some("notepad".into()), None, Some("Slack".into())]);
        session.release(&engine).unwrap();
        session.engine_replaced();
        *engine.state.borrow_mut() = DictationState::Idle;
        session.press(&engine, window("slack.exe", "")).unwrap();
        assert_eq!(engine.polish_calls().len(), 4, "a new engine is sent its polish");
    }

    #[test]
    fn toggle_while_processing_starts_the_next_dictation() {
        let (engine, mut session) = (FakeEngine::new(), Session::default());
        *engine.state.borrow_mut() = DictationState::Processing;
        let started = session.toggle(&engine, window("", "")).unwrap();
        assert!(matches!(started, Toggled::Started { ref app, .. } if app.is_empty()), "{started:?}");
        assert_eq!(engine.calls.borrow().last(), Some(&Call::Start(String::new(), None)));
    }

    #[test]
    fn a_start_error_is_returned() {
        let (mut engine, mut session) = (FakeEngine::new(), Session::default());
        engine.fail_start = true;
        let started = session.toggle(&engine, window("a.exe", ""));
        assert!(matches!(started, Err(BoloError::ModelMissing(ref id)) if id == "nemotron-stream"), "{started:?}");
    }

    #[test]
    fn press_starts_and_release_stops() {
        let (engine, mut session) = (FakeEngine::new(), Session::default());
        assert!(!session.release(&engine).unwrap(), "nothing to stop");
        let started = session.press(&engine, window("code.exe", "x")).unwrap();
        assert!(matches!(started, Toggled::Started { ref app, polish: false, .. } if app == "code.exe"), "{started:?}");
        assert_eq!(session.press(&engine, window("code.exe", "x")).unwrap(), Toggled::AlreadyListening);
        assert!(session.release(&engine).unwrap());
        assert!(!session.release(&engine).unwrap(), "already processing");
        assert_eq!(
            *engine.calls.borrow(),
            [Call::SetPolish(None), Call::Start("code.exe".into(), None), Call::Stop],
            "an editor's title stays out of the engine"
        );
    }

    #[test]
    fn press_while_paused_does_nothing() {
        let (engine, mut session) = (FakeEngine::new(), Session::default());
        session.set_paused(true, None);
        assert_eq!(session.press(&engine, never).unwrap(), Toggled::Paused);
        assert!(engine.calls.borrow().is_empty());
    }

    #[test]
    fn pausing_cancels_and_unloads_and_blocks_toggles_until_resumed() {
        let (engine, mut session) = (FakeEngine::new(), Session::default());
        session.toggle(&engine, window("a.exe", "")).unwrap();
        session.set_paused(true, Some(&engine));
        assert!(session.paused());
        assert_eq!(session.toggle(&engine, never).unwrap(), Toggled::Paused);
        assert_eq!(engine.calls.borrow()[2..], [Call::Cancel, Call::Release]);
        session.set_paused(false, Some(&engine));
        assert!(matches!(session.toggle(&engine, window("a.exe", "")), Ok(Toggled::Started { .. })));
    }
}
