//! The push-to-talk state machine (ADR-0017): key events and timer ticks in, what to do
//! with the key and which dictation command to send out.
//!
//! It is pure, with the clock passed in, so every rule is tested without a desktop. The
//! keyboard hook ([`crate::keyhook`]) runs it inside the hook procedure: it decides in
//! microseconds, takes no lock and allocates nothing, because Windows drops a low-level
//! hook that is slow to answer.
//!
//! - The trigger key (Caps Lock by default) is swallowed. Held for [`HOLD`] or more it
//!   starts a dictation, and its release stops it. Released sooner it is a tap, replayed
//!   so Caps Lock still toggles.
//! - The trigger with any modifier held passes through untouched, up and down.
//! - Esc is swallowed while a dictation is listening, and it cancels.
//! - Auto-repeat of a held key is swallowed and changes nothing.
//! - Keys Bolo sends itself (tagged, see [`crate::keyhook::BOLO_TAG`]) are ignored.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// How long the trigger must be held before a dictation starts. Shorter is a tap.
pub const HOLD: Duration = Duration::from_millis(150);

/// A key, as the hook classifies it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// The push-to-talk key from settings.
    Trigger,
    Esc,
    Other,
}

/// A keyboard event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyEvent {
    pub key: Key,
    pub down: bool,
    /// Shift, Ctrl, Alt or Win is held (the trigger itself doesn't count).
    pub modifiers: bool,
    /// Bolo sent this key itself.
    pub injected: bool,
}

impl KeyEvent {
    pub fn down(key: Key) -> Self {
        Self { key, down: true, modifiers: false, injected: false }
    }
    pub fn up(key: Key) -> Self {
        Self { key, down: false, modifiers: false, injected: false }
    }
    pub fn with_modifiers(self) -> Self {
        Self { modifiers: true, ..self }
    }
    pub fn injected(self) -> Self {
        Self { injected: true, ..self }
    }
}

/// What happens to the key event itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disposition {
    /// The app in front gets the key as usual.
    PassThrough,
    /// Nobody else sees it.
    Swallow,
}

/// What the app should do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Start,
    Stop,
    Cancel,
    /// The trigger was only tapped: send it again, tagged, so it does what it normally does.
    ReplayTap,
}

impl Command {
    pub const ALL: [Command; 4] = [Command::Start, Command::Stop, Command::Cancel, Command::ReplayTap];

    pub fn to_code(self) -> usize {
        self as usize
    }
    pub fn from_code(code: usize) -> Option<Self> {
        Self::ALL.get(code).copied()
    }
}

/// The result of one key event. A trigger released just past [`HOLD`] before a tick saw
/// it produces two commands: Start, then Stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Output {
    pub disposition: Disposition,
    pub commands: [Option<Command>; 2],
}

impl Output {
    fn pass() -> Self {
        Self { disposition: Disposition::PassThrough, commands: [None; 2] }
    }
    fn swallow(command: Option<Command>) -> Self {
        Self { disposition: Disposition::Swallow, commands: [command, None] }
    }
    pub fn commands(&self) -> impl Iterator<Item = Command> + '_ {
        self.commands.iter().flatten().copied()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Idle,
    /// The trigger is down without modifiers; not yet a hold.
    Pressed {
        since: Instant,
    },
    /// Held past [`HOLD`]: a dictation was started.
    Held,
    /// Esc cancelled the dictation while the trigger was still down.
    Cancelled,
    /// The trigger went down with a modifier; its repeats and release pass through too.
    PassingThrough,
}

#[derive(Debug, Clone)]
pub struct Machine {
    state: State,
    /// The last Esc down was swallowed, so its release is too.
    esc_swallowed: bool,
}

impl Default for Machine {
    fn default() -> Self {
        Self { state: State::Idle, esc_swallowed: false }
    }
}

impl Machine {
    /// When the next [`Machine::tick`] can do something, if ever.
    pub fn deadline(&self) -> Option<Instant> {
        match self.state {
            State::Pressed { since } => Some(since + HOLD),
            _ => None,
        }
    }

    /// Time passed: a trigger down for [`HOLD`] becomes a hold.
    pub fn tick(&mut self, now: Instant) -> Option<Command> {
        match self.state {
            State::Pressed { since } if now.duration_since(since) >= HOLD => {
                self.state = State::Held;
                Some(Command::Start)
            }
            _ => None,
        }
    }

    /// A key went down or up. `listening` is whether a dictation is listening now,
    /// however it was started (the tray can start one too).
    pub fn key(&mut self, ev: KeyEvent, now: Instant, listening: bool) -> Output {
        if ev.injected {
            return Output::pass();
        }
        match ev.key {
            Key::Other => Output::pass(),
            Key::Esc => self.esc(ev.down, listening),
            Key::Trigger => self.trigger(ev, now),
        }
    }

    fn esc(&mut self, down: bool, listening: bool) -> Output {
        if !down {
            return if std::mem::take(&mut self.esc_swallowed) { Output::swallow(None) } else { Output::pass() };
        }
        let active = listening || self.state == State::Held;
        if !active {
            // A repeat of a swallowed Esc stays swallowed.
            return if self.esc_swallowed { Output::swallow(None) } else { Output::pass() };
        }
        let first = !self.esc_swallowed;
        self.esc_swallowed = true;
        if self.state == State::Held {
            self.state = State::Cancelled;
        }
        Output::swallow(first.then_some(Command::Cancel))
    }

    fn trigger(&mut self, ev: KeyEvent, now: Instant) -> Output {
        let started = self.tick(now);
        match (self.state, ev.down) {
            (State::Idle, true) if ev.modifiers => {
                self.state = State::PassingThrough;
                Output::pass()
            }
            (State::Idle, true) => {
                self.state = State::Pressed { since: now };
                Output::swallow(None)
            }
            (State::PassingThrough, down) => {
                if !down {
                    self.state = State::Idle;
                }
                Output::pass()
            }
            // Auto-repeat.
            (State::Pressed { .. } | State::Held | State::Cancelled, true) => Output::swallow(started),
            (State::Pressed { .. }, false) => {
                self.state = State::Idle;
                Output::swallow(Some(Command::ReplayTap))
            }
            (State::Held, false) => {
                self.state = State::Idle;
                Output { disposition: Disposition::Swallow, commands: [started, Some(Command::Stop)] }
            }
            (State::Cancelled, false) => {
                self.state = State::Idle;
                Output::swallow(None)
            }
            // An up with no down seen (the hook was installed while the key was held).
            (State::Idle, false) => Output::pass(),
        }
    }
}

/// Flags shared between the hook and the rest of the app, read and written without locks.
#[derive(Debug, Default)]
pub struct Flags {
    listening: AtomicBool,
    watching: AtomicBool,
    touched: AtomicBool,
}

impl Flags {
    pub fn listening(&self) -> bool {
        self.listening.load(Ordering::Relaxed)
    }
    pub fn set_listening(&self, on: bool) {
        self.listening.store(on, Ordering::Relaxed);
    }

    /// Start watching for the user's own keys and clicks: [`Flags::touched`] turns true on
    /// the next one. Used while a polish swap is pending (ADR-0018).
    pub fn watch_touch(&self) {
        self.touched.store(false, Ordering::Relaxed);
        self.watching.store(true, Ordering::Release);
    }
    pub fn stop_watching(&self) {
        self.watching.store(false, Ordering::Release);
    }
    pub fn watching(&self) -> bool {
        self.watching.load(Ordering::Acquire)
    }
    /// A key or click came in since [`Flags::watch_touch`].
    pub fn touched(&self) -> bool {
        self.touched.load(Ordering::Relaxed)
    }
    /// The user pressed a key or clicked; counts only while watching.
    pub fn touch(&self) {
        if self.watching() {
            self.touched.store(true, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{Command, Disposition, Flags, HOLD, Key, KeyEvent, Machine, Output};

    /// A machine and a fake clock.
    struct Rig {
        m: Machine,
        t0: Instant,
        now: Instant,
        listening: bool,
    }

    impl Rig {
        fn new() -> Self {
            let t0 = Instant::now();
            Self { m: Machine::default(), t0, now: t0, listening: false }
        }
        fn at(&mut self, ms: u64) -> &mut Self {
            self.now = self.t0 + Duration::from_millis(ms);
            self
        }
        fn key(&mut self, ev: KeyEvent) -> (Disposition, Vec<Command>) {
            let out: Output = self.m.key(ev, self.now, self.listening);
            for c in out.commands() {
                match c {
                    Command::Start => self.listening = true,
                    Command::Stop | Command::Cancel => self.listening = false,
                    Command::ReplayTap => {}
                }
            }
            (out.disposition, out.commands().collect())
        }
        fn tick(&mut self) -> Option<Command> {
            let c = self.m.tick(self.now);
            if c == Some(Command::Start) {
                self.listening = true;
            }
            c
        }
    }

    use Command::*;
    use Disposition::*;

    const CAPS: Key = Key::Trigger;

    #[test]
    fn a_tap_is_swallowed_and_replayed() {
        let mut r = Rig::new();
        assert_eq!(r.key(KeyEvent::down(CAPS)), (Swallow, vec![]));
        assert_eq!(r.at(100).tick(), None);
        assert_eq!(r.at(149).key(KeyEvent::up(CAPS)), (Swallow, vec![ReplayTap]));
        assert_eq!(r.m.deadline(), None);
    }

    #[test]
    fn a_hold_starts_on_the_tick_and_stops_on_release() {
        let mut r = Rig::new();
        r.key(KeyEvent::down(CAPS));
        assert_eq!(r.m.deadline(), Some(r.t0 + HOLD));
        assert_eq!(r.at(149).tick(), None);
        assert_eq!(r.at(150).tick(), Some(Start));
        assert_eq!(r.at(151).tick(), None, "starts once");
        assert_eq!(r.at(3000).key(KeyEvent::up(CAPS)), (Swallow, vec![Stop]));
    }

    #[test]
    fn a_release_past_the_boundary_before_any_tick_starts_then_stops() {
        let mut r = Rig::new();
        r.key(KeyEvent::down(CAPS));
        assert_eq!(r.at(150).key(KeyEvent::up(CAPS)), (Swallow, vec![Start, Stop]));
    }

    #[test]
    fn auto_repeat_is_swallowed_and_changes_nothing() {
        let mut r = Rig::new();
        r.key(KeyEvent::down(CAPS));
        assert_eq!(r.at(40).key(KeyEvent::down(CAPS)), (Swallow, vec![]));
        assert_eq!(r.at(80).key(KeyEvent::down(CAPS)), (Swallow, vec![]));
        assert_eq!(r.at(120).key(KeyEvent::up(CAPS)), (Swallow, vec![ReplayTap]), "still a tap");

        r.at(1000).key(KeyEvent::down(CAPS));
        // A repeat past the boundary starts the hold if no tick has yet.
        assert_eq!(r.at(1200).key(KeyEvent::down(CAPS)), (Swallow, vec![Start]));
        assert_eq!(r.at(1300).key(KeyEvent::down(CAPS)), (Swallow, vec![]));
        assert_eq!(r.at(1400).key(KeyEvent::up(CAPS)), (Swallow, vec![Stop]));
    }

    #[test]
    fn the_trigger_with_a_modifier_passes_through_down_repeat_and_up() {
        let mut r = Rig::new();
        assert_eq!(r.key(KeyEvent::down(CAPS).with_modifiers()), (PassThrough, vec![]));
        assert_eq!(r.m.deadline(), None);
        // Shift let go first: the rest of this press still passes through.
        assert_eq!(r.at(300).key(KeyEvent::down(CAPS)), (PassThrough, vec![]));
        assert_eq!(r.at(400).tick(), None);
        assert_eq!(r.at(500).key(KeyEvent::up(CAPS)), (PassThrough, vec![]));
        // The next press without a modifier is push-to-talk again.
        assert_eq!(r.at(600).key(KeyEvent::down(CAPS)), (Swallow, vec![]));
    }

    #[test]
    fn esc_while_listening_cancels_and_is_swallowed_with_its_release() {
        let mut r = Rig::new();
        r.key(KeyEvent::down(CAPS));
        r.at(200).tick();
        assert_eq!(r.at(500).key(KeyEvent::down(Key::Esc)), (Swallow, vec![Cancel]));
        assert_eq!(r.at(530).key(KeyEvent::down(Key::Esc)), (Swallow, vec![]), "repeat");
        assert_eq!(r.at(550).key(KeyEvent::up(Key::Esc)), (Swallow, vec![]));
        // The trigger's release after a cancel doesn't stop anything.
        assert_eq!(r.at(900).key(KeyEvent::up(CAPS)), (Swallow, vec![]));
        assert_eq!(r.at(1000).key(KeyEvent::down(Key::Esc)), (PassThrough, vec![]));
    }

    #[test]
    fn esc_cancels_a_dictation_the_tray_started() {
        let mut r = Rig::new();
        r.listening = true;
        assert_eq!(r.key(KeyEvent::down(Key::Esc)), (Swallow, vec![Cancel]));
        assert_eq!(r.key(KeyEvent::up(Key::Esc)), (Swallow, vec![]));
    }

    #[test]
    fn esc_while_idle_passes_through() {
        let mut r = Rig::new();
        assert_eq!(r.key(KeyEvent::down(Key::Esc)), (PassThrough, vec![]));
        assert_eq!(r.key(KeyEvent::up(Key::Esc)), (PassThrough, vec![]));
        // Before the hold starts, Esc is not Bolo's.
        r.key(KeyEvent::down(CAPS));
        assert_eq!(r.at(100).key(KeyEvent::down(Key::Esc)), (PassThrough, vec![]));
    }

    #[test]
    fn injected_keys_are_ignored() {
        let mut r = Rig::new();
        assert_eq!(r.key(KeyEvent::down(CAPS).injected()), (PassThrough, vec![]));
        assert_eq!(r.key(KeyEvent::up(CAPS).injected()), (PassThrough, vec![]));
        assert_eq!(r.m.deadline(), None);
        r.listening = true;
        assert_eq!(r.key(KeyEvent::down(Key::Esc).injected()), (PassThrough, vec![]));
    }

    #[test]
    fn other_keys_pass_through_even_during_a_hold() {
        let mut r = Rig::new();
        r.key(KeyEvent::down(CAPS));
        r.at(200).tick();
        assert_eq!(r.key(KeyEvent::down(Key::Other)), (PassThrough, vec![]));
        assert_eq!(r.key(KeyEvent::up(Key::Other)), (PassThrough, vec![]));
    }

    #[test]
    fn a_release_with_no_press_seen_passes_through() {
        let mut r = Rig::new();
        assert_eq!(r.key(KeyEvent::up(CAPS)), (PassThrough, vec![]));
    }

    #[test]
    fn commands_round_trip_through_their_codes() {
        for c in Command::ALL {
            assert_eq!(Command::from_code(c.to_code()), Some(c));
        }
        assert_eq!(Command::from_code(99), None);
    }

    #[test]
    fn touches_count_only_while_watching() {
        let f = Flags::default();
        f.touch();
        assert!(!f.touched());
        f.watch_touch();
        assert!(!f.touched());
        f.touch();
        assert!(f.touched());
        f.watch_touch();
        assert!(!f.touched(), "a new watch starts clean");
        f.stop_watching();
        f.touch();
        assert!(!f.touched());
    }
}
