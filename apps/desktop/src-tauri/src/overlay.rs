//! The overlay pill's driver: a thread that runs the [`Pill`] state machine on the app's
//! events, its own timers and the mouse, sends its views to the pill webview, and
//! places, shows and hides the pill window.
//!
//! [`Overlay`] sits in front of the webviews' [`Emit`]: every [`AppEvent`] passes through
//! it unchanged, and the ones the pill cares about are also posted to its thread. The
//! window is created hidden at startup (`tauri.conf.json`, label `pill`), so showing it
//! costs no webview start-up.
//!
//! The idle button (ticket 15): while it's on in settings, Bolo isn't paused and no
//! fullscreen app fills the monitor in front (checked every [`POLL`]), the pill rests on
//! screen as a small bar, following the window in front to its monitor. The webview
//! reports where its clickable parts are ([`HitRegion`]); the driver maps them onto the
//! screen for the mouse hook ([`crate::pointer`]) and turns its clicks and drags into
//! [`Clicks`] and window moves.
//!
//! Seams: [`PillWindow`] for the window ([`win::WinPill`] is the real one), [`Pointer`]
//! for the mouse hook, [`Clicks`] for what the buttons do.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::controller::{AppEvent, Emit};
use crate::pill::{Effect, Input, Pill, Rect, View, place, position_of};
use crate::pointer::{Action, Region};

/// How often the idle button checks for a fullscreen app or a new monitor in front.
pub const POLL: Duration = Duration::from_millis(500);

/// A monitor, in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Monitor {
    /// Tells monitors apart (the `HMONITOR`).
    pub id: isize,
    /// The work area: the screen less the taskbar.
    pub work: Rect,
    /// DPI / 96.
    pub scale: f64,
    /// A fullscreen app (a game, a video, a presentation) fills it and is in front.
    pub fullscreen: bool,
}

/// The pill window. Calls come from the overlay's thread and must not block on the UI.
pub trait PillWindow: Send + Sync {
    /// The monitor holding the window in front.
    fn monitor(&self) -> Option<Monitor>;
    /// Place the window at `r` and show it, without activating it.
    fn show_at(&self, r: Rect);
    /// Move the shown window to `r` (a drag).
    fn move_to(&self, r: Rect);
    fn hide(&self);
}

/// The mouse hook: the pill's clickable regions on screen; none unhooks it.
pub trait Pointer: Send {
    fn set_regions(&self, regions: Vec<(Region, Rect)>, draggable: bool);
}

/// What the pill's buttons do. Called on the overlay's thread: implementations hand the
/// work off rather than block it (starting a dictation can load the model).
pub trait Clicks: Send + Sync {
    /// Start a hands-free dictation into the app in front.
    fn start(&self);
    /// Finish the hands-free dictation and insert its text.
    fn stop(&self);
    fn cancel(&self);
    /// Remember where the user dragged the pill (0–1 across the work area).
    fn save_position(&self, position: f64);
}

/// A clickable part of the pill, as the webview lays it out: CSS pixels from the
/// window's top-left.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct HitRegion {
    pub region: Region,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// The overlay's settings at startup.
#[derive(Debug, Clone)]
pub struct Config {
    pub idle_button: bool,
    /// 0–1 across the work area.
    pub position: f64,
    /// The push-to-talk key's name, for the idle hint.
    pub key: String,
}

#[derive(Debug)]
enum Msg {
    Pill(Input),
    Paused(bool),
    IdleButton(bool),
    Regions(Vec<HitRegion>),
    Pointer(Action),
}

/// Passes events to the webviews and drives the pill from them.
pub struct Overlay {
    inner: Arc<dyn Emit>,
    tx: Sender<Msg>,
    /// The view last sent, for a webview that loads after it went out.
    view: Arc<Mutex<View>>,
}

impl Overlay {
    /// Start the pill's thread. Its views go out through `inner` as [`AppEvent::Pill`];
    /// `actions` are the mouse hook's, if there is one.
    pub fn spawn(
        inner: Arc<dyn Emit>,
        window: Box<dyn PillWindow>,
        pointer: Option<(Box<dyn Pointer>, Receiver<Action>)>,
        clicks: Arc<dyn Clicks>,
        config: Config,
    ) -> Arc<Self> {
        let (tx, rx) = mpsc::channel();
        let pointer = pointer.map(|(hook, actions)| {
            let tx = tx.clone();
            let forwarded = std::thread::Builder::new().name("bolo-pointer-actions".into()).spawn(move || {
                for a in actions {
                    if tx.send(Msg::Pointer(a)).is_err() {
                        return;
                    }
                }
            });
            if let Err(e) = forwarded {
                tracing::warn!(%e, "the pill won't react to the mouse");
            }
            hook
        });
        let view = Arc::new(Mutex::new(View::Hidden));
        let emit: Arc<dyn Emit> = Arc::new(Remember { inner: inner.clone(), view: view.clone() });
        let spawned = std::thread::Builder::new().name("bolo-pill".into()).spawn(move || {
            let mut driver = Driver::new(emit, window, pointer, clicks, config);
            driver.run(&rx);
        });
        if let Err(e) = spawned {
            // Dictation works without the pill.
            tracing::warn!(%e, "could not start the overlay pill");
        }
        Arc::new(Self { inner, tx, view })
    }

    /// What the pill shows now.
    pub fn view(&self) -> View {
        self.view.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    /// A dictation started; `polish` when polished text will follow.
    pub fn begin(&self, polish: bool) {
        let _ = self.tx.send(Msg::Pill(Input::Begin { polish }));
    }

    /// Turn the idle button on or off.
    pub fn set_idle_button(&self, on: bool) {
        let _ = self.tx.send(Msg::IdleButton(on));
    }

    /// Where the webview's clickable parts are now.
    pub fn regions(&self, regions: Vec<HitRegion>) {
        let _ = self.tx.send(Msg::Regions(regions));
    }
}

impl Emit for Overlay {
    fn emit(&self, event: AppEvent) {
        if let AppEvent::Paused { paused } = event {
            let _ = self.tx.send(Msg::Paused(paused));
        } else if let Some(input) = input(&event) {
            let _ = self.tx.send(Msg::Pill(input));
        }
        self.inner.emit(event);
    }
}

/// Sends the pill's views on, remembering the last.
struct Remember {
    inner: Arc<dyn Emit>,
    view: Arc<Mutex<View>>,
}

impl Emit for Remember {
    fn emit(&self, event: AppEvent) {
        if let AppEvent::Pill { view } = &event {
            *self.view.lock().unwrap_or_else(PoisonError::into_inner) = view.clone();
        }
        self.inner.emit(event);
    }
}

/// What an app event means to the pill. Levels and partials go to the webview alone.
pub fn input(event: &AppEvent) -> Option<Input> {
    Some(match event {
        AppEvent::State { state } => Input::State(*state),
        AppEvent::Final { output } => Input::Final { empty: output.plain.trim().is_empty() },
        AppEvent::Polished { swapped, .. } => Input::Polished { swapped: *swapped },
        AppEvent::Notice { message, hint } => Input::Notice { message: message.clone(), hint: hint.clone() },
        AppEvent::Error { message } => Input::Error(message.clone()),
        AppEvent::Paused { .. } | AppEvent::Level { .. } | AppEvent::Partial { .. } | AppEvent::Pill { .. } => {
            return None;
        }
    })
}

/// The pill thread's state.
struct Driver {
    pill: Pill,
    emit: Arc<dyn Emit>,
    window: Box<dyn PillWindow>,
    pointer: Option<Box<dyn Pointer>>,
    clicks: Arc<dyn Clicks>,
    idle_button: bool,
    paused: bool,
    fullscreen: bool,
    position: f64,
    /// The monitor and rectangle the window was last placed at, while it shows.
    placed: Option<(Monitor, Rect)>,
    /// The webview's last report, in CSS pixels.
    regions: Vec<HitRegion>,
    /// Where the window was when a drag began.
    drag_from: Option<Rect>,
    next_poll: Option<Instant>,
}

impl Driver {
    fn new(
        emit: Arc<dyn Emit>,
        window: Box<dyn PillWindow>,
        pointer: Option<Box<dyn Pointer>>,
        clicks: Arc<dyn Clicks>,
        config: Config,
    ) -> Self {
        let mut d = Self {
            pill: Pill::new(&config.key),
            emit,
            window,
            pointer,
            clicks,
            idle_button: config.idle_button,
            paused: false,
            fullscreen: false,
            position: config.position.clamp(0.0, 1.0),
            placed: None,
            regions: Vec::new(),
            drag_from: None,
            next_poll: None,
        };
        d.idle_changed(Instant::now());
        d
    }

    fn run(&mut self, rx: &Receiver<Msg>) {
        loop {
            let deadline = [self.pill.deadline(), self.next_poll].into_iter().flatten().min();
            let msg = match deadline {
                None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
                Some(at) => rx.recv_timeout(at.saturating_duration_since(Instant::now())),
            };
            match msg {
                Ok(msg) => self.handle(msg, Instant::now()),
                Err(RecvTimeoutError::Timeout) => self.tick(Instant::now()),
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    }

    fn handle(&mut self, msg: Msg, now: Instant) {
        match msg {
            Msg::Pill(input) => self.pill_input(input, now),
            Msg::Paused(paused) => {
                self.paused = paused;
                self.idle_changed(now);
            }
            Msg::IdleButton(on) => {
                self.idle_button = on;
                self.idle_changed(now);
            }
            Msg::Regions(regions) => {
                self.regions = regions;
                self.send_regions();
            }
            Msg::Pointer(action) => self.pointer_action(action, now),
        }
    }

    fn tick(&mut self, now: Instant) {
        let fx = self.pill.tick(now);
        self.apply(fx);
        if self.next_poll.is_some_and(|t| now >= t) {
            self.poll(now);
        }
    }

    fn pill_input(&mut self, input: Input, now: Instant) {
        let fx = self.pill.handle(input, now);
        self.apply(fx);
    }

    /// Whether the idle button may show now, told to the pill; polling while it's on.
    fn idle_changed(&mut self, now: Instant) {
        let wanted = self.idle_button && !self.paused;
        self.next_poll = wanted.then_some(now + POLL);
        if wanted && let Some(m) = self.window.monitor() {
            self.fullscreen = m.fullscreen;
        }
        self.pill_input(Input::Idle(wanted && !self.fullscreen), now);
    }

    /// Check for a fullscreen app, and follow the window in front to another monitor.
    fn poll(&mut self, now: Instant) {
        self.next_poll = Some(now + POLL);
        let Some(m) = self.window.monitor() else { return };
        if m.fullscreen != self.fullscreen {
            self.fullscreen = m.fullscreen;
            self.pill_input(Input::Idle(!self.fullscreen), now);
        }
        let moved = self.placed.is_some_and(|(at, _)| at.id != m.id || at.work != m.work || at.scale != m.scale);
        if moved && self.drag_from.is_none() && matches!(self.pill.view(), View::Idle { .. }) {
            self.show(m);
        }
    }

    fn apply(&mut self, fx: Vec<Effect>) {
        for effect in fx {
            match effect {
                Effect::Show => {
                    if let Some(m) = self.window.monitor() {
                        self.show(m);
                    }
                }
                Effect::Render(view) => {
                    self.emit.emit(AppEvent::Pill { view });
                    // Dragging only in idle; the webview reports the new layout next.
                    self.send_regions();
                }
                Effect::Hide => {
                    self.window.hide();
                    self.placed = None;
                    self.drag_from = None;
                    self.send_regions();
                }
            }
        }
    }

    fn show(&mut self, m: Monitor) {
        let r = place(m.work, m.scale, self.position);
        self.window.show_at(r);
        self.placed = Some((m, r));
        self.send_regions();
    }

    /// The webview's regions, on screen, to the mouse hook (none while hidden).
    fn send_regions(&self) {
        let Some(pointer) = &self.pointer else { return };
        let screen = match self.placed {
            Some((m, r)) => self
                .regions
                .iter()
                .map(|h| {
                    let px = |v: f64| (v * m.scale).round() as i32;
                    let (left, top) = (r.left + px(h.x), r.top + px(h.y));
                    (h.region, Rect { left, top, right: left + px(h.w), bottom: top + px(h.h) })
                })
                .collect(),
            None => Vec::new(),
        };
        pointer.set_regions(screen, matches!(self.pill.view(), View::Idle { .. }));
    }

    fn pointer_action(&mut self, action: Action, now: Instant) {
        match action {
            Action::Hover(region) => self.pill_input(Input::Hover(region.is_some()), now),
            Action::Click(region) => match (region, self.pill.view()) {
                (Region::Pill, View::Idle { .. }) => {
                    self.pill_input(Input::HandsFree, now);
                    self.clicks.start();
                }
                (Region::Stop, View::Listening { hands_free: true }) => self.clicks.stop(),
                (Region::Cancel, View::Listening { hands_free: true }) => self.clicks.cancel(),
                _ => {}
            },
            Action::Drag { dx } => {
                let Some((m, r)) = self.placed else { return };
                let from = *self.drag_from.get_or_insert(r);
                let left = (from.left + dx).clamp(m.work.left, m.work.right - from.width());
                let to = Rect { left, right: left + from.width(), ..from };
                if to != r {
                    self.window.move_to(to);
                    self.placed = Some((m, to));
                }
            }
            Action::DragEnd { .. } => {
                self.drag_from = None;
                if let Some((m, r)) = self.placed {
                    self.position = position_of(m.work, r);
                    self.clicks.save_position(self.position);
                }
                self.send_regions();
            }
        }
    }
}

#[cfg(windows)]
pub mod win {
    //! The real pill window: a Tauri window made click-through, non-activating and kept
    //! out of the taskbar and Alt+Tab, moved and shown with plain Win32 calls on the UI
    //! thread (Tauri's own `show` would activate it).

    use tauri::{AppHandle, WebviewWindow};
    use windows_sys::Win32::Foundation::{HWND, RECT};
    use windows_sys::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
    };
    use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetClassNameW, GetForegroundWindow, GetWindowLongPtrW, GetWindowRect, HWND_TOPMOST, SW_HIDE,
        SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW, SetWindowLongPtrW, SetWindowPos, ShowWindow,
        WS_EX_APPWINDOW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    };

    use super::{Monitor, PillWindow};
    use crate::pill::Rect;

    pub struct WinPill {
        app: AppHandle,
        hwnd: isize,
    }

    impl WinPill {
        /// Make `window` the pill: click-through (Tauri's ignore-cursor sets
        /// `WS_EX_TRANSPARENT | WS_EX_LAYERED`), then `WS_EX_NOACTIVATE |
        /// WS_EX_TOOLWINDOW` without `WS_EX_APPWINDOW`, after it so they stay.
        pub fn new(app: AppHandle, window: &WebviewWindow) -> tauri::Result<Self> {
            window.set_ignore_cursor_events(true)?;
            let pill = Self { app, hwnd: window.hwnd()?.0 as isize };
            pill.on_ui(move |hwnd| {
                // SAFETY: `hwnd` is the pill's live window, on its own thread.
                unsafe {
                    let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
                    // Tauri marks it WS_EX_APPWINDOW even with skipTaskbar, and that puts it
                    // in Alt+Tab whatever else is set.
                    let ex = (ex | (WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW) as isize) & !(WS_EX_APPWINDOW as isize);
                    SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex);
                }
            });
            Ok(pill)
        }

        /// Run `f` with the window handle on the UI thread, which owns the window, so
        /// the overlay's thread never waits on it.
        fn on_ui(&self, f: impl FnOnce(HWND) + Send + 'static) {
            let hwnd = self.hwnd;
            if let Err(e) = self.app.run_on_main_thread(move || f(hwnd as HWND)) {
                tracing::warn!(%e, "could not reach the pill window");
            }
        }
    }

    impl PillWindow for WinPill {
        fn monitor(&self) -> Option<Monitor> {
            // SAFETY: Win32 queries with sized, owned out-parameters.
            unsafe {
                let fg = GetForegroundWindow();
                let monitor = MonitorFromWindow(fg, MONITOR_DEFAULTTONEAREST);
                if monitor.is_null() {
                    return None;
                }
                let mut info: MONITORINFO = std::mem::zeroed();
                info.cbSize = size_of::<MONITORINFO>() as u32;
                if GetMonitorInfoW(monitor, &mut info) == 0 {
                    return None;
                }
                let (mut dpi_x, mut dpi_y) = (96u32, 96u32);
                if GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) != 0 {
                    dpi_x = 96;
                }
                let rect = |r: RECT| Rect { left: r.left, top: r.top, right: r.right, bottom: r.bottom };
                let fullscreen = fills(fg, self.hwnd, info.rcMonitor);
                Some(Monitor {
                    id: monitor as isize,
                    work: rect(info.rcWork),
                    scale: f64::from(dpi_x) / 96.0,
                    fullscreen,
                })
            }
        }

        fn show_at(&self, r: Rect) {
            self.on_ui(move |hwnd| {
                // SAFETY: plain window calls on our own window, on its thread.
                unsafe {
                    // Onto the monitor first: crossing to one with another DPI rescales
                    // the window, so the final size is set once it's there.
                    SetWindowPos(hwnd, HWND_TOPMOST, r.left, r.top, 0, 0, SWP_NOACTIVATE | SWP_NOSIZE);
                    SetWindowPos(
                        hwnd,
                        HWND_TOPMOST,
                        r.left,
                        r.top,
                        r.width(),
                        r.height(),
                        SWP_NOACTIVATE | SWP_SHOWWINDOW,
                    );
                }
            });
        }

        fn move_to(&self, r: Rect) {
            self.on_ui(move |hwnd| {
                // SAFETY: as above.
                unsafe {
                    SetWindowPos(hwnd, HWND_TOPMOST, r.left, r.top, 0, 0, SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOZORDER);
                }
            });
        }

        fn hide(&self) {
            self.on_ui(|hwnd| {
                // SAFETY: as above.
                unsafe {
                    ShowWindow(hwnd, SW_HIDE);
                }
            });
        }
    }

    /// Whether `fg` is an app covering the whole of `monitor` (fullscreen), as opposed to
    /// the desktop, the taskbar or the pill itself.
    fn fills(fg: HWND, pill: isize, monitor: RECT) -> bool {
        if fg.is_null() || fg as isize == pill {
            return false;
        }
        // SAFETY: sized buffers and an owned RECT.
        unsafe {
            let mut class = [0u16; 64];
            let n = GetClassNameW(fg, class.as_mut_ptr(), class.len() as i32).max(0) as usize;
            let class = String::from_utf16_lossy(&class[..n]);
            if matches!(class.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd") {
                return false;
            }
            let mut r: RECT = std::mem::zeroed();
            GetWindowRect(fg, &mut r) != 0
                && r.left <= monitor.left
                && r.top <= monitor.top
                && r.right >= monitor.right
                && r.bottom >= monitor.bottom
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use bolo_core::{DictationState, FormattedOutput, LanguageMode, Timings};

    use super::*;
    use crate::controller::testing::Recorder;

    /// Everything the fakes saw, in order.
    #[derive(Default)]
    struct Log(Mutex<Vec<String>>);

    impl Log {
        fn push(&self, s: impl Into<String>) {
            self.0.lock().unwrap().push(s.into());
        }
        fn take(&self) -> Vec<String> {
            std::mem::take(&mut *self.0.lock().unwrap())
        }
    }

    struct FakeWindow {
        log: Arc<Log>,
        monitor: Arc<Mutex<Monitor>>,
    }

    impl PillWindow for FakeWindow {
        fn monitor(&self) -> Option<Monitor> {
            Some(*self.monitor.lock().unwrap())
        }
        fn show_at(&self, r: Rect) {
            self.log.push(format!("show {} {}", r.left, r.top));
        }
        fn move_to(&self, r: Rect) {
            self.log.push(format!("move {}", r.left));
        }
        fn hide(&self) {
            self.log.push("hide");
        }
    }

    struct FakePointer(Arc<Log>);

    impl Pointer for FakePointer {
        fn set_regions(&self, regions: Vec<(Region, Rect)>, draggable: bool) {
            let r: Vec<String> = regions.iter().map(|(g, r)| format!("{g:?}@{},{}", r.left, r.top)).collect();
            self.0.push(format!("regions [{}] drag={draggable}", r.join(" ")));
        }
    }

    struct FakeClicks(Arc<Log>);

    impl Clicks for FakeClicks {
        fn start(&self) {
            self.0.push("start");
        }
        fn stop(&self) {
            self.0.push("stop");
        }
        fn cancel(&self) {
            self.0.push("cancel");
        }
        fn save_position(&self, position: f64) {
            self.0.push(format!("save {position:.3}"));
        }
    }

    const WORK: Rect = Rect { left: 0, top: 0, right: 1920, bottom: 1032 };

    struct Rig {
        rec: Arc<Recorder>,
        log: Arc<Log>,
        monitor: Arc<Mutex<Monitor>>,
        d: Driver,
        now: Instant,
    }

    fn rig(idle_button: bool) -> Rig {
        rig_on(idle_button, Monitor { id: 1, work: WORK, scale: 1.0, fullscreen: false })
    }

    fn rig_on(idle_button: bool, monitor: Monitor) -> Rig {
        let rec = Arc::new(Recorder::default());
        let log = Arc::new(Log::default());
        let monitor = Arc::new(Mutex::new(monitor));
        let d = Driver::new(
            rec.clone(),
            Box::new(FakeWindow { log: log.clone(), monitor: monitor.clone() }),
            Some(Box::new(FakePointer(log.clone()))),
            Arc::new(FakeClicks(log.clone())),
            Config { idle_button, position: 0.5, key: "Caps Lock".into() },
        );
        Rig { rec, log, monitor, d, now: Instant::now() }
    }

    impl Rig {
        fn msg(&mut self, m: Msg) {
            self.d.handle(m, self.now);
        }
        fn state(&mut self, s: DictationState) {
            self.msg(Msg::Pill(Input::State(s)));
        }
        fn wait(&mut self, ms: u64) {
            self.now += Duration::from_millis(ms);
            self.d.tick(self.now);
        }
        fn views(&self) -> Vec<View> {
            self.rec
                .take()
                .into_iter()
                .filter_map(|e| match e {
                    AppEvent::Pill { view } => Some(view),
                    _ => None,
                })
                .collect()
        }
    }

    fn idle(hover: bool) -> View {
        View::Idle { hover, key: "Caps Lock".into() }
    }

    fn output(text: &str) -> crate::controller::Output {
        FormattedOutput {
            plain: text.into(),
            markdown: text.into(),
            html: None,
            timings: Timings::default(),
            engine_id: "fake".into(),
            language: LanguageMode::English,
        }
        .into()
    }

    #[test]
    fn events_map_to_pill_inputs() {
        assert_eq!(
            input(&AppEvent::State { state: DictationState::Listening }),
            Some(Input::State(DictationState::Listening))
        );
        assert_eq!(input(&AppEvent::Final { output: output("  ") }), Some(Input::Final { empty: true }));
        assert_eq!(input(&AppEvent::Final { output: output("Hi.") }), Some(Input::Final { empty: false }));
        assert_eq!(
            input(&AppEvent::Polished { output: output("Hi!"), swapped: true }),
            Some(Input::Polished { swapped: true })
        );
        assert_eq!(input(&AppEvent::Level { rms: 0.1 }), None);
        assert_eq!(input(&AppEvent::Partial { text: "hi".into() }), None);
        assert_eq!(input(&AppEvent::Pill { view: View::Listening { hands_free: false } }), None);
    }

    #[test]
    fn with_the_idle_button_on_it_shows_at_startup_at_the_saved_spot() {
        let r = rig(true);
        assert_eq!(r.views(), [idle(false)]);
        assert_eq!(r.log.take(), ["show 680 952", "regions [] drag=true", "regions [] drag=true"]);
    }

    #[test]
    fn with_it_off_nothing_shows_until_a_dictation() {
        let mut r = rig(false);
        assert_eq!(r.views(), []);
        assert_eq!(r.d.next_poll, None, "and nothing polls");
        r.state(DictationState::Listening);
        assert_eq!(r.views(), [View::Listening { hands_free: false }]);
    }

    #[test]
    fn regions_are_mapped_onto_the_screen_at_the_monitor_scale() {
        let mut r = rig_on(true, Monitor { id: 1, work: WORK, scale: 1.5, fullscreen: false });
        r.log.take();
        r.msg(Msg::Regions(vec![HitRegion { region: Region::Pill, x: 244.0, y: 40.0, w: 72.0, h: 28.0 }]));
        // place() at 150%: 840×108, left (1920 − 840) / 2 = 540, top 1032 − 12 − 108 = 912.
        assert_eq!(r.log.take(), ["regions [Pill@906,972] drag=true"]);
    }

    #[test]
    fn a_click_on_the_idle_button_starts_hands_free_and_its_buttons_finish_it() {
        let mut r = rig(true);
        r.views();
        r.log.take();
        r.msg(Msg::Pointer(Action::Click(Region::Pill)));
        assert_eq!(r.log.take(), ["start"]);
        r.state(DictationState::Listening);
        assert_eq!(r.views(), [View::Listening { hands_free: true }]);
        r.log.take();
        r.msg(Msg::Pointer(Action::Click(Region::Stop)));
        r.msg(Msg::Pointer(Action::Click(Region::Cancel)));
        assert_eq!(r.log.take(), ["stop", "cancel"]);
    }

    #[test]
    fn clicks_do_nothing_in_other_states() {
        let mut r = rig(true);
        r.state(DictationState::Listening);
        r.log.take();
        r.msg(Msg::Pointer(Action::Click(Region::Pill)));
        r.msg(Msg::Pointer(Action::Click(Region::Stop)));
        assert_eq!(r.log.take(), Vec::<String>::new(), "the key's dictation has no buttons");
    }

    #[test]
    fn hover_grows_the_button() {
        let mut r = rig(true);
        r.views();
        r.msg(Msg::Pointer(Action::Hover(Some(Region::Pill))));
        r.msg(Msg::Pointer(Action::Hover(None)));
        assert_eq!(r.views(), [idle(true), idle(false)]);
    }

    #[test]
    fn dragging_moves_the_window_within_the_work_area_and_saves_the_spot() {
        let mut r = rig(true);
        r.log.take();
        r.msg(Msg::Pointer(Action::Drag { dx: -200 }));
        r.msg(Msg::Pointer(Action::Drag { dx: -2000 }));
        r.msg(Msg::Pointer(Action::DragEnd { dx: -2000 }));
        let log = r.log.take();
        assert_eq!(&log[..2], ["move 480", "move 0"]);
        assert_eq!(log[2], "save 0.146", "the window's centre, 280 px of 1920");
        // The next show puts it there.
        r.msg(Msg::IdleButton(false));
        r.wait(140);
        r.msg(Msg::IdleButton(true));
        assert!(r.log.take().contains(&"show 0 952".to_string()));
    }

    #[test]
    fn pausing_or_a_fullscreen_app_hides_the_button() {
        let mut r = rig(true);
        r.views();
        r.msg(Msg::Paused(true));
        assert_eq!(r.views(), [View::Hidden]);
        assert_eq!(r.d.next_poll, None, "no polling while paused");
        r.msg(Msg::Paused(false));
        assert_eq!(r.views(), [idle(false)]);
        r.monitor.lock().unwrap().fullscreen = true;
        r.wait(500);
        assert_eq!(r.views(), [View::Hidden]);
        r.monitor.lock().unwrap().fullscreen = false;
        r.wait(500);
        assert_eq!(r.views(), [idle(false)]);
    }

    #[test]
    fn the_button_follows_the_window_in_front_to_another_monitor() {
        let mut r = rig(true);
        r.log.take();
        *r.monitor.lock().unwrap() = Monitor {
            id: 2,
            work: Rect { left: 1920, top: 0, right: 3840, bottom: 1032 },
            scale: 1.0,
            fullscreen: false,
        };
        r.wait(500);
        assert_eq!(r.log.take()[0], "show 2600 952");
    }

    #[test]
    fn the_overlay_thread_forwards_everything_and_drives_the_window() {
        let rec = Arc::new(Recorder::default());
        let log = Arc::new(Log::default());
        let monitor = Arc::new(Mutex::new(Monitor { id: 1, work: WORK, scale: 1.0, fullscreen: false }));
        let overlay = Overlay::spawn(
            rec.clone(),
            Box::new(FakeWindow { log: log.clone(), monitor }),
            None,
            Arc::new(FakeClicks(log.clone())),
            Config { idle_button: false, position: 0.5, key: "Caps Lock".into() },
        );
        overlay.emit(AppEvent::State { state: DictationState::Listening });
        overlay.begin(false);
        overlay.emit(AppEvent::Level { rms: 0.1 });
        overlay.emit(AppEvent::State { state: DictationState::Processing });
        overlay.emit(AppEvent::Final { output: output("Hi.") });
        // Inserted hides after 600 ms, and the window after the 140 ms exit.
        let until = Instant::now() + Duration::from_secs(5);
        while !log.0.lock().unwrap().contains(&"hide".to_string()) && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(5));
        }
        let events = rec.take();
        assert!(events.contains(&AppEvent::Level { rms: 0.1 }), "levels pass straight through");
        let views: Vec<View> = events
            .into_iter()
            .filter_map(|e| match e {
                AppEvent::Pill { view } => Some(view),
                _ => None,
            })
            .collect();
        assert_eq!(views, [View::Listening { hands_free: false }, View::Inserted, View::Hidden]);
        assert_eq!(log.take(), ["show 680 952", "hide"]);
        assert_eq!(overlay.view(), View::Hidden, "the last view sent");
    }

    #[test]
    fn a_late_webview_can_ask_for_the_view() {
        let log = Arc::new(Log::default());
        let monitor = Arc::new(Mutex::new(Monitor { id: 1, work: WORK, scale: 1.0, fullscreen: false }));
        let overlay = Overlay::spawn(
            Arc::new(Recorder::default()),
            Box::new(FakeWindow { log: log.clone(), monitor }),
            None,
            Arc::new(FakeClicks(log)),
            Config { idle_button: true, position: 0.5, key: "Caps Lock".into() },
        );
        let until = Instant::now() + Duration::from_secs(5);
        while overlay.view() == View::Hidden && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(overlay.view(), idle(false));
    }
}
