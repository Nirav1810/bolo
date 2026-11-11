//! Starting, stopping and pausing dictations: what the tray and the push-to-talk key ask
//! for, decided here so it's testable without a desktop.

use bolo_core::{BoloError, DictationState, Engine, TargetContext};

use crate::desktop::Desktop;

/// The engine methods the session drives. [`Engine`] is the real one.
pub trait EngineControl {
    fn start(&self, ctx: TargetContext) -> Result<(), BoloError>;
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
    Started {
        app: String,
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
}

impl Session {
    pub fn paused(&self) -> bool {
        self.paused
    }

    /// Stop the dictation that is listening, or start one for the window in front.
    pub fn toggle(&self, engine: &dyn EngineControl, desktop: &dyn Desktop) -> Result<Toggled, BoloError> {
        if self.paused {
            return Ok(Toggled::Paused);
        }
        if engine.state() == DictationState::Listening {
            engine.stop()?;
            return Ok(Toggled::Stopped);
        }
        let ctx = target_context(desktop);
        let app = ctx.app_id.clone();
        engine.start(ctx)?;
        Ok(Toggled::Started { app })
    }

    /// The push-to-talk key was held: start a dictation for the window in front. One
    /// already listening (started from the tray) carries on, and the key's release stops it.
    ///
    /// It doesn't look at [`EngineControl::state`] first: a stop sent just before is still
    /// queued, and the engine runs its commands in order, so a start after it succeeds.
    pub fn press(&self, engine: &dyn EngineControl, desktop: &dyn Desktop) -> Result<Toggled, BoloError> {
        if self.paused {
            return Ok(Toggled::Paused);
        }
        let ctx = target_context(desktop);
        let app = ctx.app_id.clone();
        match engine.start(ctx) {
            Ok(()) => Ok(Toggled::Started { app }),
            Err(BoloError::Busy) => Ok(Toggled::AlreadyListening),
            Err(e) => Err(e),
        }
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
}

/// The target context for the window in front. Plain text for every app until the app
/// rules land (ticket 07).
pub fn target_context(desktop: &dyn Desktop) -> TargetContext {
    let fg = desktop.foreground().unwrap_or_default();
    TargetContext { app_id: fg.exe, window_title: (!fg.title.is_empty()).then_some(fg.title), ..Default::default() }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use bolo_core::{BoloError, DictationState, OutputFormat, TargetContext};

    use super::{EngineControl, Session, Toggled};
    use crate::desktop::fake::FakeDesktop;

    #[derive(Debug, PartialEq)]
    enum Call {
        Start(String, Option<String>),
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
    }

    impl EngineControl for FakeEngine {
        fn start(&self, ctx: TargetContext) -> Result<(), BoloError> {
            assert_eq!(ctx.format, OutputFormat::Plain);
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

    #[test]
    fn toggle_starts_for_the_window_in_front_then_stops() {
        let (engine, desktop, session) =
            (FakeEngine::new(), FakeDesktop::in_front("notepad.exe", "notes.txt - Notepad"), Session::default());
        assert_eq!(session.toggle(&engine, &desktop).unwrap(), Toggled::Started { app: "notepad.exe".into() });
        assert_eq!(session.toggle(&engine, &desktop).unwrap(), Toggled::Stopped);
        assert_eq!(
            *engine.calls.borrow(),
            [Call::Start("notepad.exe".into(), Some("notes.txt - Notepad".into())), Call::Stop]
        );
    }

    #[test]
    fn toggle_while_processing_starts_the_next_dictation() {
        let (engine, desktop, session) = (FakeEngine::new(), FakeDesktop::default(), Session::default());
        *engine.state.borrow_mut() = DictationState::Processing;
        assert_eq!(session.toggle(&engine, &desktop).unwrap(), Toggled::Started { app: String::new() });
        assert_eq!(*engine.calls.borrow(), [Call::Start(String::new(), None)]);
    }

    #[test]
    fn a_start_error_is_returned() {
        let (mut engine, desktop, session) = (FakeEngine::new(), FakeDesktop::default(), Session::default());
        engine.fail_start = true;
        assert!(
            matches!(session.toggle(&engine, &desktop), Err(BoloError::ModelMissing(id)) if id == "nemotron-stream")
        );
    }

    #[test]
    fn press_starts_and_release_stops() {
        let (engine, desktop, session) =
            (FakeEngine::new(), FakeDesktop::in_front("code.exe", "x"), Session::default());
        assert!(!session.release(&engine).unwrap(), "nothing to stop");
        assert_eq!(session.press(&engine, &desktop).unwrap(), Toggled::Started { app: "code.exe".into() });
        assert_eq!(session.press(&engine, &desktop).unwrap(), Toggled::AlreadyListening);
        assert!(session.release(&engine).unwrap());
        assert!(!session.release(&engine).unwrap(), "already processing");
        assert_eq!(*engine.calls.borrow(), [Call::Start("code.exe".into(), Some("x".into())), Call::Stop]);
    }

    #[test]
    fn press_while_paused_does_nothing() {
        let (engine, desktop, mut session) = (FakeEngine::new(), FakeDesktop::default(), Session::default());
        session.set_paused(true, None);
        assert_eq!(session.press(&engine, &desktop).unwrap(), Toggled::Paused);
        assert!(engine.calls.borrow().is_empty());
    }

    #[test]
    fn pausing_cancels_and_unloads_and_blocks_toggles_until_resumed() {
        let (engine, desktop, mut session) = (FakeEngine::new(), FakeDesktop::default(), Session::default());
        session.toggle(&engine, &desktop).unwrap();
        session.set_paused(true, Some(&engine));
        assert!(session.paused());
        assert_eq!(session.toggle(&engine, &desktop).unwrap(), Toggled::Paused);
        assert_eq!(engine.calls.borrow()[1..], [Call::Cancel, Call::Release]);
        session.set_paused(false, Some(&engine));
        assert!(matches!(session.toggle(&engine, &desktop), Ok(Toggled::Started { .. })));
    }
}
