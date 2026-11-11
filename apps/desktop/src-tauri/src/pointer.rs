//! The mouse over the overlay pill (M3 ticket 15).
//!
//! The pill window is click-through and never activated: WebView2 takes focus when
//! clicked, and Bolo must leave focus in the app it pastes into. So a `WH_MOUSE_LL` hook
//! ([`win::PointerHook`]), installed only while the pill shows, sees the mouse instead.
//! [`Gesture`] is its pure core: it hit-tests the pill's regions (which the webview
//! reports, see [`crate::overlay`]) and turns left-button presses on them into hover,
//! click and drag. Presses on a region are swallowed, so the app underneath never gets
//! them; everything else passes through untouched.

use serde::{Deserialize, Serialize};

use crate::pill::Rect;

/// The parts of the pill that react to the mouse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Region {
    /// The whole pill (in idle, the bar and the space around it).
    Pill,
    /// Hands-free: finish and insert.
    Stop,
    /// Hands-free: drop the dictation.
    Cancel,
}

/// A point on the screen, in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

/// What the left button and the mouse did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Move(Point),
    Down(Point),
    Up(Point),
}

/// What a mouse event means for the pill.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// The region under the mouse changed.
    Hover(Option<Region>),
    /// Pressed and released on the same region without dragging.
    Click(Region),
    /// Dragging the pill, `dx` pixels from where the press began.
    Drag { dx: i32 },
    /// The drag ended, `dx` pixels from where it began.
    DragEnd { dx: i32 },
}

/// How far the mouse moves with the button down before a press becomes a drag.
pub const DRAG_SLOP: i32 = 4;

#[derive(Debug, Clone, Copy)]
struct Press {
    region: Region,
    start: Point,
    dragging: bool,
}

#[derive(Debug, Default)]
pub struct Gesture {
    /// Screen rectangles, buttons before the pill so they win.
    regions: Vec<(Region, Rect)>,
    /// Whether pressing the pill and moving drags it.
    draggable: bool,
    hover: Option<Region>,
    press: Option<Press>,
}

impl Gesture {
    /// Replace the regions (empty while the pill is hidden). A press in progress
    /// carries on.
    pub fn set_regions(&mut self, mut regions: Vec<(Region, Rect)>, draggable: bool) {
        regions.sort_by_key(|(r, _)| *r == Region::Pill);
        self.regions = regions;
        self.draggable = draggable;
    }

    fn hit(&self, p: Point) -> Option<Region> {
        self.regions
            .iter()
            .find(|(_, r)| p.x >= r.left && p.x < r.right && p.y >= r.top && p.y < r.bottom)
            .map(|(region, _)| *region)
    }

    /// Feed an event. Returns whether to swallow it, and what it meant.
    pub fn event(&mut self, ev: Event) -> (bool, Vec<Action>) {
        let mut actions = Vec::new();
        let swallow = match ev {
            Event::Move(p) => {
                if let Some(press) = self.press.as_mut() {
                    let dx = p.x - press.start.x;
                    if press.region == Region::Pill && self.draggable && (press.dragging || dx.abs() > DRAG_SLOP) {
                        press.dragging = true;
                        actions.push(Action::Drag { dx });
                    }
                } else {
                    self.hover_to(self.hit(p), &mut actions);
                }
                false
            }
            Event::Down(p) => match self.hit(p) {
                Some(region) => {
                    self.press = Some(Press { region, start: p, dragging: false });
                    true
                }
                None => false,
            },
            Event::Up(p) => match self.press.take() {
                Some(press) => {
                    if press.dragging {
                        actions.push(Action::DragEnd { dx: p.x - press.start.x });
                    } else if self.hit(p) == Some(press.region) {
                        actions.push(Action::Click(press.region));
                    }
                    self.hover_to(self.hit(p), &mut actions);
                    true
                }
                None => false,
            },
        };
        (swallow, actions)
    }

    fn hover_to(&mut self, region: Option<Region>, actions: &mut Vec<Action>) {
        if self.hover != region {
            self.hover = region;
            actions.push(Action::Hover(region));
        }
    }
}

#[cfg(windows)]
pub mod win {
    //! The `WH_MOUSE_LL` hook: a thread with a message loop that installs the hook while
    //! the pill shows and runs [`Gesture`] inside the callback, which must decide
    //! whether to swallow before it returns. Actions go out on a channel.

    use std::cell::RefCell;
    use std::ptr::null_mut;
    use std::sync::mpsc::{self, Receiver, Sender};
    use std::thread::JoinHandle;

    use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, GetMessageW, HHOOK, MSG, MSLLHOOKSTRUCT, PM_NOREMOVE, PeekMessageW, PostThreadMessageW,
        SetWindowsHookExW, UnhookWindowsHookEx, WH_MOUSE_LL, WM_APP, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
        WM_QUIT,
    };

    use super::{Action, Event, Gesture, Point, Region};
    use crate::pill::Rect;

    /// New regions are waiting on the channel.
    const WM_REGIONS: u32 = WM_APP + 1;

    struct State {
        gesture: Gesture,
        actions: Sender<Action>,
    }

    thread_local! {
        static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
    }

    type Regions = (Vec<(Region, Rect)>, bool);

    pub struct PointerHook {
        thread_id: u32,
        regions: Sender<Regions>,
        thread: Option<JoinHandle<()>>,
    }

    impl PointerHook {
        /// Start the hook's thread. Actions arrive on the returned receiver.
        pub fn start() -> std::io::Result<(Self, Receiver<Action>)> {
            let (actions_tx, actions_rx) = mpsc::channel();
            let (regions_tx, regions_rx) = mpsc::channel::<Regions>();
            let (ready_tx, ready_rx) = mpsc::channel();
            let thread = std::thread::Builder::new()
                .name("bolo-pointer".into())
                .spawn(move || run(actions_tx, &regions_rx, &ready_tx))?;
            let thread_id = ready_rx.recv().map_err(std::io::Error::other)?;
            Ok((Self { thread_id, regions: regions_tx, thread: Some(thread) }, actions_rx))
        }

        /// The pill's regions on screen; empty unhooks the mouse.
        pub fn set_regions(&self, regions: Vec<(Region, Rect)>, draggable: bool) {
            if self.regions.send((regions, draggable)).is_ok() {
                // SAFETY: posting to our own thread's queue.
                unsafe { PostThreadMessageW(self.thread_id, WM_REGIONS, 0, 0) };
            }
        }
    }

    impl crate::overlay::Pointer for PointerHook {
        fn set_regions(&self, regions: Vec<(Region, Rect)>, draggable: bool) {
            PointerHook::set_regions(self, regions, draggable);
        }
    }

    impl Drop for PointerHook {
        fn drop(&mut self) {
            // SAFETY: as above.
            unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0) };
            if let Some(t) = self.thread.take() {
                let _ = t.join();
            }
        }
    }

    fn run(actions: Sender<Action>, regions: &Receiver<Regions>, ready: &Sender<u32>) {
        STATE.with_borrow_mut(|s| *s = Some(State { gesture: Gesture::default(), actions }));
        // SAFETY: the usual hook thread: create the queue, then pump it; the hook is
        // removed before the thread ends.
        unsafe {
            let mut msg: MSG = std::mem::zeroed();
            PeekMessageW(&mut msg, null_mut(), 0, 0, PM_NOREMOVE);
            let _ = ready.send(GetCurrentThreadId());
            let module = GetModuleHandleW(std::ptr::null());
            let mut hook: HHOOK = null_mut();
            while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
                if msg.message != WM_REGIONS {
                    continue;
                }
                let Some((latest, draggable)) = regions.try_iter().last() else { continue };
                let on = !latest.is_empty();
                STATE.with_borrow_mut(|s| {
                    if let Some(s) = s {
                        s.gesture.set_regions(latest, draggable);
                    }
                });
                if on && hook.is_null() {
                    hook = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), module, 0);
                    if hook.is_null() {
                        tracing::warn!(e = %std::io::Error::last_os_error(), "could not hook the mouse for the pill");
                    }
                } else if !on && !hook.is_null() {
                    UnhookWindowsHookEx(hook);
                    hook = null_mut();
                }
            }
            if !hook.is_null() {
                UnhookWindowsHookEx(hook);
            }
        }
        STATE.with_borrow_mut(|s| *s = None);
    }

    unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0 {
            // SAFETY: for WH_MOUSE_LL, lparam points at an MSLLHOOKSTRUCT.
            let info = unsafe { &*(lparam as *const MSLLHOOKSTRUCT) };
            let p = Point { x: info.pt.x, y: info.pt.y };
            let event = match wparam as u32 {
                WM_MOUSEMOVE => Some(Event::Move(p)),
                WM_LBUTTONDOWN => Some(Event::Down(p)),
                WM_LBUTTONUP => Some(Event::Up(p)),
                _ => None,
            };
            if let Some(ev) = event {
                let swallow = STATE.with(|cell| {
                    let Ok(mut state) = cell.try_borrow_mut() else { return false };
                    let Some(s) = state.as_mut() else { return false };
                    let (swallow, actions) = s.gesture.event(ev);
                    for a in actions {
                        let _ = s.actions.send(a);
                    }
                    swallow
                });
                if swallow {
                    return 1;
                }
            }
        }
        // SAFETY: passing the event on, as the hook contract requires.
        unsafe { CallNextHookEx(null_mut(), code, wparam, lparam) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(left: i32, top: i32, right: i32, bottom: i32) -> Rect {
        Rect { left, top, right, bottom }
    }

    fn pt(x: i32, y: i32) -> Point {
        Point { x, y }
    }

    /// A hands-free pill at 100..300 × 0..40, with ✕ and ✓ buttons at its right end.
    fn hands_free() -> Gesture {
        let mut g = Gesture::default();
        g.set_regions(
            vec![
                (Region::Pill, rect(100, 0, 300, 40)),
                (Region::Cancel, rect(240, 8, 264, 32)),
                (Region::Stop, rect(268, 8, 292, 32)),
            ],
            false,
        );
        g
    }

    fn idle() -> Gesture {
        let mut g = Gesture::default();
        g.set_regions(vec![(Region::Pill, rect(100, 0, 172, 28))], true);
        g
    }

    #[test]
    fn moving_over_the_pill_hovers_it_and_nothing_is_swallowed() {
        let mut g = idle();
        assert_eq!(g.event(Event::Move(pt(50, 10))), (false, vec![]));
        assert_eq!(g.event(Event::Move(pt(120, 10))), (false, vec![Action::Hover(Some(Region::Pill))]));
        assert_eq!(g.event(Event::Move(pt(130, 10))), (false, vec![]), "only changes are reported");
        assert_eq!(g.event(Event::Move(pt(500, 10))), (false, vec![Action::Hover(None)]));
    }

    #[test]
    fn a_press_and_release_on_the_pill_is_a_swallowed_click() {
        let mut g = idle();
        g.event(Event::Move(pt(120, 10)));
        assert_eq!(g.event(Event::Down(pt(120, 10))), (true, vec![]));
        assert_eq!(g.event(Event::Up(pt(121, 11))), (true, vec![Action::Click(Region::Pill)]));
    }

    #[test]
    fn presses_elsewhere_pass_through() {
        let mut g = idle();
        assert_eq!(g.event(Event::Down(pt(10, 10))), (false, vec![]));
        assert_eq!(g.event(Event::Up(pt(10, 10))), (false, vec![]));
    }

    #[test]
    fn buttons_win_over_the_pill_around_them() {
        let mut g = hands_free();
        g.event(Event::Down(pt(250, 20)));
        assert_eq!(
            g.event(Event::Up(pt(250, 20))).1,
            [Action::Click(Region::Cancel), Action::Hover(Some(Region::Cancel))]
        );
        g.event(Event::Down(pt(280, 20)));
        assert_eq!(g.event(Event::Up(pt(280, 20))).1, [Action::Click(Region::Stop), Action::Hover(Some(Region::Stop))]);
    }

    #[test]
    fn releasing_off_the_pressed_button_is_no_click_but_still_swallowed() {
        let mut g = hands_free();
        g.event(Event::Down(pt(280, 20)));
        let (swallow, actions) = g.event(Event::Up(pt(150, 20)));
        assert!(swallow, "the app underneath never saw the press, so not the release either");
        assert_eq!(actions, [Action::Hover(Some(Region::Pill))]);
    }

    #[test]
    fn dragging_the_idle_pill_past_the_slop() {
        let mut g = idle();
        g.event(Event::Down(pt(120, 10)));
        assert_eq!(g.event(Event::Move(pt(123, 30))), (false, vec![]), "within the slop");
        assert_eq!(g.event(Event::Move(pt(140, 30))), (false, vec![Action::Drag { dx: 20 }]));
        assert_eq!(g.event(Event::Move(pt(122, 30))), (false, vec![Action::Drag { dx: 2 }]), "a drag stays a drag");
        let (swallow, actions) = g.event(Event::Up(pt(90, 30)));
        assert!(swallow);
        assert_eq!(actions[0], Action::DragEnd { dx: -30 });
        assert!(!actions.contains(&Action::Click(Region::Pill)), "a drag is no click");
    }

    #[test]
    fn the_hands_free_pill_does_not_drag() {
        let mut g = hands_free();
        g.event(Event::Down(pt(120, 20)));
        assert_eq!(g.event(Event::Move(pt(200, 20))), (false, vec![]));
    }

    #[test]
    fn with_no_regions_nothing_is_touched() {
        let mut g = idle();
        g.set_regions(vec![], false);
        assert_eq!(g.event(Event::Down(pt(120, 10))), (false, vec![]));
    }

    #[test]
    fn regions_serialize_lower_case() {
        assert_eq!(serde_json::to_value(Region::Stop).unwrap(), "stop");
        assert_eq!(serde_json::from_value::<Region>("pill".into()).unwrap(), Region::Pill);
    }
}
