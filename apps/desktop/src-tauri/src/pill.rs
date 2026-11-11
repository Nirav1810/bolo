//! The overlay pill's state machine (M3 ticket 08, design in ticket 04).
//!
//! [`Pill`] takes what happens to a dictation ([`Input`]) and the clock, and says what
//! the pill shows ([`View`]) and when its window appears and goes ([`Effect`]). It is
//! pure, with the clock passed in, so every timing rule is tested here; the
//! [`crate::overlay`] thread drives it and does the window work.
//!
//! - Listening at Start, with the waveform and the partial (drawn by the webview).
//! - Transcribing only when the text takes longer than [`TRANSCRIBING_AFTER`].
//! - Inserted, or Polishing while a polish is expected. Polishing ends as Polished when
//!   the swap happens, as Inserted when it is skipped, and gives up after
//!   [`POLISH_WAIT`] (polish that fails or times out sends nothing).
//! - A notice or an error stays [`NOTICE_FOR`]; the rest hide [`HIDE_AFTER`] after
//!   they are reached.
//! - Hiding plays the webview's exit, and the window goes [`EXIT`] later.
//! - While the idle button is allowed (ticket 15: on in settings, Bolo not paused, no
//!   fullscreen app in front), the pill rests as [`View::Idle`] instead of hiding. A
//!   click on it starts a hands-free dictation, whose Listening has Stop and Cancel.
//!
//! [`place`] is where the window goes: along the bottom of the monitor of the window in
//! front, in that monitor's physical pixels.

use std::time::{Duration, Instant};

use bolo_core::{DictationState, POLISH_DEADLINE};
use serde::Serialize;

/// "Transcribing" shows only when the text takes longer than this after release.
pub const TRANSCRIBING_AFTER: Duration = Duration::from_millis(250);
/// Inserted, Polished and Cancelled stay this long.
pub const HIDE_AFTER: Duration = Duration::from_millis(600);
/// Notices and errors stay this long.
pub const NOTICE_FOR: Duration = Duration::from_secs(4);
/// The webview's exit animation; the window hides once it has played.
pub const EXIT: Duration = Duration::from_millis(140);
/// How long Polishing waits for the polished text. Polish that fails, times out or
/// fails its guard sends nothing, so this ends it; past the engine's deadline the
/// polished text can't come.
pub const POLISH_WAIT: Duration = POLISH_DEADLINE.saturating_add(Duration::from_millis(500));

/// What the pill shows. Sent to the pill webview as `AppEvent::Pill`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum View {
    Hidden,
    /// The idle button: a small bar, grown into a hint on hover. `key` names the
    /// push-to-talk key for the hint.
    Idle {
        hover: bool,
        key: String,
    },
    /// The webview draws the waveform and the partial from the level and partial events.
    /// Hands-free (started with a click) adds Stop and Cancel.
    Listening {
        hands_free: bool,
    },
    Transcribing,
    Inserted,
    Polishing,
    Polished,
    Notice {
        message: String,
        hint: Option<String>,
    },
    Error {
        message: String,
    },
    Cancelled,
}

/// What happens to a dictation, as far as the pill cares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    /// A dictation started; `polish` says whether polished text will follow its text.
    Begin {
        polish: bool,
    },
    State(DictationState),
    /// The instant text arrived; `empty` when nothing was heard.
    Final {
        empty: bool,
    },
    /// The polished text arrived; `swapped` when it replaced the instant text.
    Polished {
        swapped: bool,
    },
    Notice {
        message: String,
        hint: Option<String>,
    },
    Error(String),
    /// Whether the idle button may show.
    Idle(bool),
    /// The mouse moved onto (or off) the pill.
    Hover(bool),
    /// The next dictation was started with a click, not the key.
    HandsFree,
    /// The push-to-talk key changed in settings; this is its name for the idle hint.
    Key(String),
}

/// What the overlay does, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// Place the window on the monitor in front and show it, without activating it.
    Show,
    /// Send the view to the webview.
    Render(View),
    /// Hide the window (its exit has played).
    Hide,
}

#[derive(Debug)]
pub struct Pill {
    view: View,
    /// Whether the window is showing (or about to hide).
    shown: bool,
    /// Stopped and waiting for the text.
    processing: bool,
    expect_polish: bool,
    transcribing_at: Option<Instant>,
    /// When the current view ends and the pill hides.
    view_until: Option<Instant>,
    /// When the window hides, after the exit.
    hide_at: Option<Instant>,
    /// Whether the idle button may show; then the pill rests as Idle instead of hiding.
    idle: bool,
    hover: bool,
    hands_free: bool,
    /// The push-to-talk key's name, for the idle hint.
    key: String,
}

impl Default for Pill {
    fn default() -> Self {
        Self::new("Caps Lock")
    }
}

impl Pill {
    pub fn new(key: &str) -> Self {
        Self {
            view: View::Hidden,
            shown: false,
            processing: false,
            expect_polish: false,
            transcribing_at: None,
            view_until: None,
            hide_at: None,
            idle: false,
            hover: false,
            hands_free: false,
            key: key.to_owned(),
        }
    }

    fn listening(&self) -> bool {
        matches!(self.view, View::Listening { .. })
    }

    pub fn view(&self) -> &View {
        &self.view
    }

    /// The next time [`Self::tick`] has something to do.
    pub fn deadline(&self) -> Option<Instant> {
        [self.transcribing_at, self.view_until, self.hide_at].into_iter().flatten().min()
    }

    pub fn handle(&mut self, input: Input, now: Instant) -> Vec<Effect> {
        let mut fx = Vec::new();
        match input {
            Input::Begin { polish } => self.expect_polish = polish,
            Input::State(DictationState::Listening) => {
                self.processing = false;
                self.transcribing_at = None;
                // Shown again even if showing: the window in front may be elsewhere.
                self.shown = false;
                let hands_free = std::mem::take(&mut self.hands_free);
                self.set(View::Listening { hands_free }, None, now, &mut fx);
            }
            Input::State(DictationState::Processing) => {
                if self.listening() {
                    self.processing = true;
                    self.transcribing_at = Some(now + TRANSCRIBING_AFTER);
                }
            }
            Input::State(DictationState::Idle) => {
                let processing = std::mem::take(&mut self.processing);
                self.transcribing_at = None;
                match self.view {
                    // Idle straight from listening: cancelled (Esc, or Bolo paused).
                    View::Listening { .. } if !processing => self.set(View::Cancelled, Some(HIDE_AFTER), now, &mut fx),
                    // Stopped, and no text came (nor an error to show).
                    View::Listening { .. } | View::Transcribing => self.set(View::Hidden, None, now, &mut fx),
                    _ => {}
                }
            }
            Input::State(DictationState::Done) => {}
            Input::Final { empty } => {
                self.processing = false;
                self.transcribing_at = None;
                let polish = std::mem::take(&mut self.expect_polish);
                match self.view {
                    // The paste was blocked or failed: that notice stays.
                    View::Notice { .. } | View::Error { .. } => {}
                    _ if empty => self.set(View::Hidden, None, now, &mut fx),
                    _ if polish => self.set(View::Polishing, Some(POLISH_WAIT), now, &mut fx),
                    _ => self.set(View::Inserted, Some(HIDE_AFTER), now, &mut fx),
                }
            }
            Input::Polished { swapped } => {
                if self.view == View::Polishing {
                    // Skipped: the instant text stands.
                    let view = if swapped { View::Polished } else { View::Inserted };
                    self.set(view, Some(HIDE_AFTER), now, &mut fx);
                }
            }
            Input::Notice { message, hint } => {
                self.set(View::Notice { message, hint }, Some(NOTICE_FOR), now, &mut fx);
            }
            Input::Error(message) => {
                self.processing = false;
                self.transcribing_at = None;
                self.set(View::Error { message }, Some(NOTICE_FOR), now, &mut fx);
            }
            Input::Idle(idle) => {
                if self.idle != idle {
                    self.idle = idle;
                    match self.view {
                        // Resting either way: show the button, or let it go.
                        View::Hidden | View::Idle { .. } => self.set(View::Hidden, None, now, &mut fx),
                        _ => {}
                    }
                }
            }
            Input::Hover(hover) => {
                self.hover = hover;
                if matches!(self.view, View::Idle { .. }) {
                    self.set(View::Hidden, None, now, &mut fx);
                }
            }
            Input::HandsFree => self.hands_free = true,
            Input::Key(key) => {
                self.key = key;
                if matches!(self.view, View::Idle { .. }) {
                    self.set(View::Hidden, None, now, &mut fx);
                }
            }
        }
        fx
    }

    pub fn tick(&mut self, now: Instant) -> Vec<Effect> {
        let mut fx = Vec::new();
        if self.transcribing_at.is_some_and(|t| now >= t) {
            self.transcribing_at = None;
            if self.processing && self.listening() {
                self.set(View::Transcribing, None, now, &mut fx);
            }
        }
        if self.view_until.is_some_and(|t| now >= t) {
            self.set(View::Hidden, None, now, &mut fx);
        }
        if self.hide_at.is_some_and(|t| now >= t) {
            self.hide_at = None;
            self.shown = false;
            fx.push(Effect::Hide);
        }
        fx
    }

    /// Show `view`, for `stay` if given. Hidden rests as Idle while the idle button is
    /// allowed; otherwise it plays the exit, then hides the window.
    fn set(&mut self, view: View, stay: Option<Duration>, now: Instant, fx: &mut Vec<Effect>) {
        self.view_until = stay.map(|d| now + d);
        if view == View::Hidden {
            self.processing = false;
            self.transcribing_at = None;
        }
        let view = match view {
            View::Hidden if self.idle => View::Idle { hover: self.hover, key: self.key.clone() },
            v => v,
        };
        if view == View::Hidden {
            if self.view != View::Hidden {
                self.hide_at = Some(now + EXIT);
            }
        } else {
            self.hide_at = None;
            if !self.shown {
                self.shown = true;
                fx.push(Effect::Show);
            }
        }
        if self.view != view {
            self.view = view.clone();
            fx.push(Effect::Render(view));
        }
    }
}

/// The pill window's size in logical pixels: the widest pill (520) with room for its
/// shadow and its entrance (it rises 8 px).
pub const WINDOW_WIDTH: f64 = 560.0;
pub const WINDOW_HEIGHT: f64 = 72.0;
/// The pill sits this far above the bottom of the monitor's work area...
pub const OFFSET: f64 = 24.0;
/// ...and this far above the bottom of its window (the shadow's room).
pub const BOTTOM_MARGIN: f64 = 16.0;

/// A rectangle in physical pixels: left, top, right, bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn width(&self) -> i32 {
        self.right - self.left
    }
    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }
}

/// The pill window's rectangle on a monitor whose work area (the screen less the
/// taskbar) is `work`, at `scale` (DPI / 96). Along the bottom, so the pill is
/// [`OFFSET`] above the work area, centred at `position` across it (0 left, 0.5 centre,
/// 1 right) but never past its edges, nor wider than it.
pub fn place(work: Rect, scale: f64, position: f64) -> Rect {
    let px = |logical: f64| (logical * scale).round() as i32;
    let width = px(WINDOW_WIDTH).min(work.width());
    let height = px(WINDOW_HEIGHT);
    let centre = work.left + (f64::from(work.width()) * position.clamp(0.0, 1.0)).round() as i32;
    let left = (centre - width / 2).clamp(work.left, work.right - width);
    let bottom = work.bottom - px(OFFSET - BOTTOM_MARGIN);
    Rect { left, top: bottom - height, right: left + width, bottom }
}

/// The `position` that [`place`] would put a window of `rect`'s width at, so that a
/// window dragged to `rect` stays there.
pub fn position_of(work: Rect, rect: Rect) -> f64 {
    if work.width() <= 0 {
        return 0.5;
    }
    let centre = f64::from(rect.left - work.left) + f64::from(rect.width()) / 2.0;
    (centre / f64::from(work.width())).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use bolo_core::DictationState::{Done, Idle, Listening, Processing};

    use super::*;

    /// A pill and a clock that only moves when told.
    struct Rig {
        pill: Pill,
        t0: Instant,
        now: Instant,
    }

    impl Rig {
        fn new() -> Self {
            let t0 = Instant::now();
            Self { pill: Pill::default(), t0, now: t0 }
        }
        fn on(&mut self, input: Input) -> Vec<Effect> {
            self.pill.handle(input, self.now)
        }
        fn state(&mut self, s: DictationState) -> Vec<Effect> {
            self.on(Input::State(s))
        }
        /// Move the clock on by `ms`, running every deadline passed on the way.
        fn wait(&mut self, ms: u64) -> Vec<Effect> {
            let end = self.now + Duration::from_millis(ms);
            let mut fx = Vec::new();
            while let Some(d) = self.pill.deadline().filter(|d| *d <= end) {
                self.now = self.now.max(d);
                fx.extend(self.pill.tick(self.now));
            }
            self.now = end;
            fx
        }
        fn at_ms(&self) -> u128 {
            (self.now - self.t0).as_millis()
        }
        /// Listening, then released.
        fn dictate(&mut self, polish: bool) {
            self.state(Listening);
            self.on(Input::Begin { polish });
            self.state(Processing);
        }
        /// The engine's events for text that arrives at once.
        fn text(&mut self, empty: bool) -> Vec<Effect> {
            let mut fx = self.on(Input::Final { empty });
            fx.extend(self.state(Done));
            fx.extend(self.state(Idle));
            fx
        }
    }

    fn render(v: View) -> Effect {
        Effect::Render(v)
    }

    #[test]
    fn starts_hidden_with_nothing_to_do() {
        let p = Pill::default();
        assert_eq!(p.view(), &View::Hidden);
        assert_eq!(p.deadline(), None);
    }

    #[test]
    fn listening_shows_the_window_before_the_view() {
        let mut r = Rig::new();
        assert_eq!(r.state(Listening), [Effect::Show, render(View::Listening { hands_free: false })]);
        assert_eq!(r.on(Input::Begin { polish: false }), []);
        assert_eq!(r.pill.deadline(), None, "listening has no time limit");
    }

    #[test]
    fn quick_text_goes_straight_to_inserted_then_hides() {
        let mut r = Rig::new();
        r.dictate(false);
        r.wait(120);
        assert_eq!(r.text(false), [render(View::Inserted)]);
        assert_eq!(r.wait(599), []);
        assert_eq!(r.wait(1), [render(View::Hidden)]);
        assert_eq!(r.wait(139), []);
        assert_eq!(r.wait(1), [Effect::Hide], "the window goes once the exit has played");
        assert_eq!(r.at_ms(), 120 + 600 + 140);
        assert_eq!(r.pill.deadline(), None);
    }

    #[test]
    fn transcribing_shows_only_past_250_ms() {
        let mut r = Rig::new();
        r.dictate(false);
        assert_eq!(r.wait(249), []);
        assert_eq!(r.wait(1), [render(View::Transcribing)]);
        assert_eq!(r.wait(500), [], "and stays until the text comes");
        assert_eq!(r.text(false), [render(View::Inserted)]);
    }

    #[test]
    fn polish_shows_polishing_then_polished() {
        let mut r = Rig::new();
        r.dictate(true);
        assert_eq!(r.text(false), [render(View::Polishing)]);
        r.wait(900);
        assert_eq!(r.on(Input::Polished { swapped: true }), [render(View::Polished)]);
        assert_eq!(r.wait(600), [render(View::Hidden)]);
    }

    #[test]
    fn a_skipped_swap_ends_as_inserted() {
        let mut r = Rig::new();
        r.dictate(true);
        r.text(false);
        assert_eq!(r.on(Input::Polished { swapped: false }), [render(View::Inserted)]);
        assert_eq!(r.wait(600), [render(View::Hidden)]);
    }

    #[test]
    fn polish_that_never_comes_gives_up_after_the_deadline() {
        let mut r = Rig::new();
        r.dictate(true);
        r.text(false);
        let wait = POLISH_WAIT.as_millis() as u64;
        assert!(wait > POLISH_DEADLINE.as_millis() as u64);
        assert_eq!(r.wait(wait - 1), []);
        assert_eq!(r.wait(1), [render(View::Hidden)]);
        // A late polish (it can't come, but if it did) changes nothing.
        assert_eq!(r.on(Input::Polished { swapped: true }), []);
    }

    #[test]
    fn polish_is_expected_only_for_the_dictation_that_asked() {
        let mut r = Rig::new();
        r.dictate(true);
        r.text(false);
        r.dictate(false);
        assert_eq!(r.text(false), [render(View::Inserted)]);
    }

    #[test]
    fn nothing_heard_hides_without_inserted() {
        let mut r = Rig::new();
        r.dictate(true);
        assert_eq!(r.text(true), [render(View::Hidden)]);
        assert_eq!(r.wait(140), [Effect::Hide]);
    }

    #[test]
    fn esc_while_listening_shows_cancelled() {
        let mut r = Rig::new();
        r.state(Listening);
        r.wait(2000);
        assert_eq!(r.state(Idle), [render(View::Cancelled)]);
        assert_eq!(r.wait(600), [render(View::Hidden)]);
        assert_eq!(r.wait(140), [Effect::Hide]);
    }

    #[test]
    fn a_blocked_paste_keeps_its_notice_over_the_text() {
        let mut r = Rig::new();
        r.dictate(true);
        let notice = View::Notice { message: "Copied".into(), hint: Some("Ctrl+V".into()) };
        // The controller sends the notice, then the final.
        assert_eq!(r.on(Input::Notice { message: "Copied".into(), hint: Some("Ctrl+V".into()) }), [render(notice)]);
        assert_eq!(r.text(false), []);
        assert_eq!(r.on(Input::Polished { swapped: false }), [], "and no polishing either");
        assert_eq!(r.wait(3999), []);
        assert_eq!(r.wait(1), [render(View::Hidden)]);
    }

    #[test]
    fn a_notice_after_the_text_replaces_inserted() {
        let mut r = Rig::new();
        r.dictate(false);
        r.text(false);
        let fx = r.on(Input::Notice { message: "hm".into(), hint: None });
        assert_eq!(fx, [render(View::Notice { message: "hm".into(), hint: None })]);
        assert_eq!(r.wait(600), [], "a notice stays 4 s, not 600 ms");
    }

    #[test]
    fn an_error_while_processing_stays_through_idle() {
        let mut r = Rig::new();
        r.dictate(false);
        r.wait(300);
        assert_eq!(
            r.on(Input::Error("no speech engine".into())),
            [render(View::Error { message: "no speech engine".into() })]
        );
        assert_eq!(r.state(Idle), []);
        assert_eq!(r.wait(4000), [render(View::Hidden)]);
    }

    #[test]
    fn an_error_with_the_pill_hidden_shows_it() {
        let mut r = Rig::new();
        assert_eq!(r.on(Input::Error("push-to-talk is unavailable".into()))[0], Effect::Show);
    }

    #[test]
    fn a_stop_with_no_text_and_no_error_hides() {
        let mut r = Rig::new();
        r.dictate(false);
        r.wait(300);
        assert_eq!(r.state(Idle), [render(View::Hidden)]);
    }

    #[test]
    fn a_new_dictation_during_the_exit_keeps_the_window() {
        let mut r = Rig::new();
        r.dictate(false);
        r.text(false);
        r.wait(600 + 70);
        assert_eq!(r.pill.view(), &View::Hidden);
        assert_eq!(r.state(Listening), [Effect::Show, render(View::Listening { hands_free: false })]);
        assert_eq!(r.wait(1000), [], "the pending hide was dropped");
    }

    #[test]
    fn a_new_dictation_while_inserted_is_placed_again() {
        let mut r = Rig::new();
        r.dictate(false);
        r.text(false);
        r.wait(200);
        assert_eq!(r.state(Listening), [Effect::Show, render(View::Listening { hands_free: false })]);
        assert_eq!(r.wait(5000), [], "and the old hide timer is gone");
    }

    #[test]
    fn transcribing_is_not_armed_when_stopping_from_another_view() {
        let mut r = Rig::new();
        r.on(Input::Error("mic".into()));
        assert_eq!(r.state(Processing), []);
        assert_eq!(r.pill.deadline(), Some(r.now + NOTICE_FOR));
    }

    fn idle(hover: bool) -> View {
        View::Idle { hover, key: "Caps Lock".into() }
    }

    #[test]
    fn the_idle_button_shows_when_allowed_and_goes_when_not() {
        let mut r = Rig::new();
        assert_eq!(r.on(Input::Idle(true)), [Effect::Show, render(idle(false))]);
        assert_eq!(r.pill.deadline(), None, "idle has no time limit");
        assert_eq!(r.on(Input::Idle(true)), [], "no change");
        assert_eq!(r.on(Input::Idle(false)), [render(View::Hidden)]);
        assert_eq!(r.wait(140), [Effect::Hide]);
    }

    #[test]
    fn hover_grows_the_idle_button() {
        let mut r = Rig::new();
        r.on(Input::Idle(true));
        assert_eq!(r.on(Input::Hover(true)), [render(idle(true))]);
        assert_eq!(r.on(Input::Hover(false)), [render(idle(false))]);
    }

    #[test]
    fn a_new_key_shows_in_the_idle_hint() {
        let mut r = Rig::new();
        r.on(Input::Idle(true));
        assert_eq!(
            r.on(Input::Key("Right Ctrl".into())),
            [render(View::Idle { hover: false, key: "Right Ctrl".into() })]
        );
    }

    #[test]
    fn with_the_idle_button_a_dictation_ends_back_at_idle_not_hidden() {
        let mut r = Rig::new();
        r.on(Input::Idle(true));
        assert_eq!(r.state(Listening), [Effect::Show, render(View::Listening { hands_free: false })]);
        r.on(Input::Begin { polish: false });
        r.state(Processing);
        assert_eq!(r.text(false), [render(View::Inserted)]);
        assert_eq!(r.wait(600), [render(idle(false))]);
        assert_eq!(r.wait(5000), [], "and the window stays");
    }

    #[test]
    fn hover_during_a_dictation_waits_for_idle() {
        let mut r = Rig::new();
        r.on(Input::Idle(true));
        r.state(Listening);
        assert_eq!(r.on(Input::Hover(true)), []);
        r.state(Idle);
        assert_eq!(r.wait(600), [render(idle(true))], "still hovered when it comes back");
    }

    #[test]
    fn going_fullscreen_mid_dictation_hides_only_afterwards() {
        let mut r = Rig::new();
        r.on(Input::Idle(true));
        r.state(Listening);
        assert_eq!(r.on(Input::Idle(false)), [], "the dictation keeps its pill");
        r.state(Idle);
        assert_eq!(r.wait(600), [render(View::Hidden)]);
        assert_eq!(r.wait(140), [Effect::Hide]);
    }

    #[test]
    fn a_click_started_dictation_is_hands_free_once() {
        let mut r = Rig::new();
        r.on(Input::Idle(true));
        r.on(Input::HandsFree);
        assert_eq!(r.state(Listening)[1], render(View::Listening { hands_free: true }));
        r.state(Idle);
        assert_eq!(r.state(Listening)[1], render(View::Listening { hands_free: false }), "the key's is not");
    }

    fn rect(left: i32, top: i32, right: i32, bottom: i32) -> Rect {
        Rect { left, top, right, bottom }
    }

    #[test]
    fn the_window_sits_bottom_centre_with_the_pill_24_px_above_the_work_area() {
        // 1920×1080 at 100% with a 48 px taskbar.
        let r = place(rect(0, 0, 1920, 1032), 1.0, 0.5);
        assert_eq!(r, rect(680, 1032 - 8 - 72, 1240, 1024));
        let pill_bottom = r.bottom - BOTTOM_MARGIN as i32;
        assert_eq!(1032 - pill_bottom, 24);
    }

    #[test]
    fn placement_scales_with_the_monitor_dpi() {
        // 2560×1440 at 150%, as a second monitor to the right of the first.
        let work = rect(1920, 0, 4480, 1368);
        let r = place(work, 1.5, 0.5);
        assert_eq!((r.width(), r.height()), (840, 108));
        assert_eq!(r.left - work.left, work.right - r.right, "centred");
        assert_eq!(work.bottom - r.bottom, 12);
    }

    #[test]
    fn the_window_is_never_wider_than_the_work_area() {
        let r = place(rect(-800, 0, 0, 600), 1.75, 0.5);
        assert_eq!((r.left, r.right), (-800, 0));
    }

    #[test]
    fn position_moves_the_window_along_the_bottom_within_the_work_area() {
        let work = rect(0, 0, 1920, 1032);
        assert_eq!(place(work, 1.0, 0.25).left, 480 - 280);
        assert_eq!(place(work, 1.0, 0.0).left, 0, "never past the left edge");
        assert_eq!(place(work, 1.0, 1.0).right, 1920, "nor the right");
        assert_eq!(place(work, 1.0, 0.25).bottom, place(work, 1.0, 0.5).bottom);
    }

    #[test]
    fn a_dragged_window_keeps_its_place() {
        let work = rect(1920, 0, 4480, 1368);
        for p in [0.2, 0.5, 0.8] {
            let r = place(work, 1.5, p);
            let back = position_of(work, r);
            assert!((back - p).abs() < 0.001, "{p} -> {back}");
            assert_eq!(place(work, 1.5, back), r);
        }
    }

    #[test]
    fn views_serialize_for_the_webview() {
        let json = |v: View| serde_json::to_value(v).unwrap();
        assert_eq!(
            json(View::Listening { hands_free: true }),
            serde_json::json!({"state": "listening", "hands_free": true})
        );
        assert_eq!(
            json(View::Idle { hover: false, key: "Caps Lock".into() }),
            serde_json::json!({"state": "idle", "hover": false, "key": "Caps Lock"})
        );
        assert_eq!(
            json(View::Notice { message: "m".into(), hint: Some("Ctrl+V".into()) }),
            serde_json::json!({"state": "notice", "message": "m", "hint": "Ctrl+V"})
        );
        assert_eq!(json(View::Error { message: "e".into() }), serde_json::json!({"state": "error", "message": "e"}));
    }
}
