//! The overlay pill's driver: a thread that runs the [`Pill`] state machine on the app's
//! events and its own timers, sends its views to the pill webview, and shows and hides
//! the pill window.
//!
//! [`Overlay`] sits in front of the webviews' [`Emit`]: every [`AppEvent`] passes through
//! it unchanged, and the ones the pill cares about are also posted to its thread. The
//! window is created hidden at startup (`tauri.conf.json`, label `pill`), so showing it
//! costs no webview start-up. [`PillWindow`] is the seam to it; [`win::WinPill`] places
//! and shows it without ever activating it.

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::Instant;

use crate::controller::{AppEvent, Emit};
use crate::pill::{Effect, Input, Pill};

/// The pill window. Calls come from the overlay's thread and must not block on the UI.
pub trait PillWindow: Send + Sync {
    /// Place it on the monitor of the window in front and show it, not activated.
    fn show(&self);
    fn hide(&self);
}

/// Passes events to the webviews and drives the pill from them.
pub struct Overlay {
    inner: Arc<dyn Emit>,
    tx: Sender<Input>,
}

impl Overlay {
    /// Start the pill's thread. Its views go out through `inner` as [`AppEvent::Pill`].
    pub fn spawn(inner: Arc<dyn Emit>, window: Box<dyn PillWindow>) -> Arc<Self> {
        let (tx, rx) = mpsc::channel();
        let emit = inner.clone();
        let spawned = std::thread::Builder::new().name("bolo-pill".into()).spawn(move || run(&rx, &*emit, &*window));
        if let Err(e) = spawned {
            // Dictation works without the pill.
            tracing::warn!(%e, "could not start the overlay pill");
        }
        Arc::new(Self { inner, tx })
    }

    /// A dictation started; `polish` when polished text will follow.
    pub fn begin(&self, polish: bool) {
        let _ = self.tx.send(Input::Begin { polish });
    }
}

impl Emit for Overlay {
    fn emit(&self, event: AppEvent) {
        if let Some(input) = input(&event) {
            let _ = self.tx.send(input);
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

/// The pill's thread: inputs as they come, timers in between, until the app goes.
fn run(rx: &Receiver<Input>, emit: &dyn Emit, window: &dyn PillWindow) {
    let mut pill = Pill::default();
    loop {
        let fx = match pill.deadline() {
            None => match rx.recv() {
                Ok(input) => pill.handle(input, Instant::now()),
                Err(_) => return,
            },
            Some(at) => match rx.recv_timeout(at.saturating_duration_since(Instant::now())) {
                Ok(input) => pill.handle(input, Instant::now()),
                Err(RecvTimeoutError::Timeout) => pill.tick(Instant::now()),
                Err(RecvTimeoutError::Disconnected) => return,
            },
        };
        for effect in fx {
            match effect {
                Effect::Show => window.show(),
                Effect::Render(view) => emit.emit(AppEvent::Pill { view }),
                Effect::Hide => window.hide(),
            }
        }
    }
}

#[cfg(windows)]
pub mod win {
    //! The real pill window: a Tauri window made click-through, non-activating and kept
    //! out of the taskbar and Alt+Tab, moved and shown with plain Win32 calls on the UI
    //! thread (Tauri's own `show` would activate it).

    use std::sync::atomic::{AtomicIsize, Ordering};

    use tauri::{AppHandle, WebviewWindow};
    use windows_sys::Win32::Foundation::{HWND, RECT};
    use windows_sys::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
    };
    use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetForegroundWindow, GetWindowLongPtrW, HWND_TOPMOST, SW_HIDE, SWP_NOACTIVATE, SWP_NOSIZE,
        SWP_SHOWWINDOW, SetWindowLongPtrW, SetWindowPos, ShowWindow, WS_EX_APPWINDOW, WS_EX_NOACTIVATE,
        WS_EX_TOOLWINDOW,
    };

    use super::PillWindow;
    use crate::pill::{Rect, place};

    pub struct WinPill {
        app: AppHandle,
        hwnd: AtomicIsize,
    }

    impl WinPill {
        /// Make `window` the pill: click-through (Tauri's ignore-cursor sets
        /// `WS_EX_TRANSPARENT | WS_EX_LAYERED`), then `WS_EX_NOACTIVATE |
        /// WS_EX_TOOLWINDOW` without `WS_EX_APPWINDOW`, after it so they stay.
        pub fn new(app: AppHandle, window: &WebviewWindow) -> tauri::Result<Self> {
            window.set_ignore_cursor_events(true)?;
            let hwnd = window.hwnd()?.0 as isize;
            let pill = Self { app, hwnd: AtomicIsize::new(hwnd) };
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
            let hwnd = self.hwnd.load(Ordering::Relaxed);
            if let Err(e) = self.app.run_on_main_thread(move || f(hwnd as HWND)) {
                tracing::warn!(%e, "could not reach the pill window");
            }
        }
    }

    impl PillWindow for WinPill {
        fn show(&self) {
            self.on_ui(|hwnd| {
                let Some((work, scale)) = foreground_monitor() else { return };
                let r = place(work, scale);
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

        fn hide(&self) {
            self.on_ui(|hwnd| {
                // SAFETY: as above.
                unsafe {
                    ShowWindow(hwnd, SW_HIDE);
                }
            });
        }
    }

    /// The work area and scale of the monitor holding the window in front (the primary
    /// monitor when nothing is in front).
    fn foreground_monitor() -> Option<(Rect, f64)> {
        // SAFETY: Win32 queries with a sized, owned MONITORINFO.
        unsafe {
            let monitor = MonitorFromWindow(GetForegroundWindow(), MONITOR_DEFAULTTONEAREST);
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
            let RECT { left, top, right, bottom } = info.rcWork;
            Some((Rect { left, top, right, bottom }, f64::from(dpi_x) / 96.0))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use bolo_core::{DictationState, FormattedOutput, LanguageMode, Timings};

    use super::*;
    use crate::controller::testing::Recorder;
    use crate::pill::View;

    #[derive(Default)]
    struct FakeWindow(Mutex<Vec<&'static str>>);

    impl PillWindow for Arc<FakeWindow> {
        fn show(&self) {
            self.0.lock().unwrap().push("show");
        }
        fn hide(&self) {
            self.0.lock().unwrap().push("hide");
        }
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
        assert_eq!(input(&AppEvent::Pill { view: View::Listening }), None);
    }

    /// Wait for `rec` to hold an event matching `pred`, with a generous limit.
    fn wait_for(rec: &Recorder, pred: impl Fn(&[AppEvent]) -> bool) -> Vec<AppEvent> {
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            let events = rec.0.lock().unwrap().clone();
            if pred(&events) || Instant::now() > until {
                return events;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn the_overlay_forwards_everything_and_drives_the_window() {
        let rec = Arc::new(Recorder::default());
        let window = Arc::new(FakeWindow::default());
        let overlay = Overlay::spawn(rec.clone(), Box::new(window.clone()));
        overlay.emit(AppEvent::State { state: DictationState::Listening });
        overlay.begin(false);
        overlay.emit(AppEvent::Level { rms: 0.1 });
        overlay.emit(AppEvent::State { state: DictationState::Processing });
        overlay.emit(AppEvent::Final { output: output("Hi.") });
        let pills = |events: &[AppEvent]| -> Vec<View> {
            events
                .iter()
                .filter_map(|e| match e {
                    AppEvent::Pill { view } => Some(view.clone()),
                    _ => None,
                })
                .collect()
        };
        // Inserted hides after 600 ms, and the window after the 140 ms exit.
        let events = wait_for(&rec, |e| pills(e).contains(&View::Hidden));
        assert_eq!(pills(&events), [View::Listening, View::Inserted, View::Hidden]);
        assert!(events.contains(&AppEvent::Level { rms: 0.1 }), "levels pass straight through");
        let until = Instant::now() + Duration::from_secs(5);
        while window.0.lock().unwrap().len() < 2 && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(*window.0.lock().unwrap(), ["show", "hide"]);
    }
}
