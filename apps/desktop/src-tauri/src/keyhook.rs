//! The low-level keyboard hook that feeds [`crate::hotkey::Machine`] (ADR-0017).
//!
//! [`KeyHook::install`] starts two threads:
//!
//! - **The hook thread** owns the `WH_KEYBOARD_LL` hook and a message loop. The hook
//!   procedure classifies the key, runs the machine (no lock, no allocation) and returns
//!   whether to swallow it. Commands are posted back to the same thread's queue as
//!   `WM_APP` messages and forwarded from the loop. The loop also wakes at the machine's
//!   deadline to tick it, and installs a `WH_MOUSE_LL` hook while a polish swap is
//!   watched for touches (see [`Flags::watch_touch`]).
//! - **The command thread** receives the commands and runs them: the dictation commands
//!   go to the [`Handler`], and a tap is replayed with `SendInput`. Starting a dictation
//!   can block on a model load, which must never stall the hook thread.
//!
//! Dropping the [`KeyHook`] unhooks and joins both threads. "Pause Bolo" does that.

use bolo_store::PushToTalkKey;

use crate::hotkey::Command;

/// `dwExtraInfo` on every key Bolo sends, so the hook ignores its own keystrokes.
pub const BOLO_TAG: usize = 0xB010_B010;

/// Runs the dictation commands the key produces. Called on the command thread.
pub trait Handler: Send + 'static {
    fn command(&self, command: Command);
}

/// The virtual-key code and whether it is an extended key, for the push-to-talk key.
pub fn virtual_key(key: PushToTalkKey) -> (u16, bool) {
    use PushToTalkKey::*;
    match key {
        CapsLock => (0x14, false),
        RightCtrl => (0xA3, true),
        RightAlt => (0xA5, true),
        F13 => (0x7C, false),
        F14 => (0x7D, false),
        F15 => (0x7E, false),
        F16 => (0x7F, false),
        F17 => (0x80, false),
        F18 => (0x81, false),
        F19 => (0x82, false),
        F20 => (0x83, false),
        F21 => (0x84, false),
        F22 => (0x85, false),
        F23 => (0x86, false),
        F24 => (0x87, false),
    }
}

#[cfg(windows)]
pub use win::KeyHook;

/// Tell the hook thread `thread_id` ([`KeyHook::thread_id`]) that [`Flags::watching`]
/// changed, so it adds or removes the mouse hook. It only posts a message, so it's safe
/// from any thread and takes no lock; 0 (no hook) does nothing.
///
/// [`Flags::watching`]: crate::hotkey::Flags::watching
pub fn watch_changed(thread_id: u32) {
    #[cfg(windows)]
    if thread_id != 0 {
        win::post_watch(thread_id);
    }
    #[cfg(not(windows))]
    let _ = thread_id;
}

#[cfg(not(windows))]
pub struct KeyHook;

#[cfg(not(windows))]
impl KeyHook {
    pub fn install(
        _: PushToTalkKey,
        _: std::sync::Arc<crate::hotkey::Flags>,
        _: impl Handler,
    ) -> std::io::Result<Self> {
        Err(std::io::Error::other("push-to-talk needs Windows"))
    }
    pub fn thread_id(&self) -> u32 {
        0
    }
}

#[cfg(windows)]
mod win {
    use std::cell::RefCell;
    use std::ptr::null_mut;
    use std::sync::Arc;
    use std::sync::mpsc;
    use std::thread::JoinHandle;
    use std::time::Instant;

    use bolo_store::PushToTalkKey;
    use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::System::Threading::{GetCurrentThreadId, INFINITE};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP,
        SendInput, VK_ESCAPE, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_RWIN,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, HHOOK, KBDLLHOOKSTRUCT, LLKHF_INJECTED, MSG, MsgWaitForMultipleObjects,
        PM_NOREMOVE, PM_REMOVE, PeekMessageW, PostThreadMessageW, QS_ALLINPUT, SetWindowsHookExW, TranslateMessage,
        UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_APP, WM_KEYDOWN, WM_LBUTTONDOWN, WM_MBUTTONDOWN, WM_QUIT,
        WM_RBUTTONDOWN, WM_SYSKEYDOWN, WM_XBUTTONDOWN,
    };

    use super::{BOLO_TAG, Handler, virtual_key};
    use crate::hotkey::{Command, Disposition, Flags, Key, KeyEvent, Machine};

    /// A command for the command thread: `wParam` is [`Command::to_code`].
    const WM_COMMAND: u32 = WM_APP + 1;
    /// Install or remove the mouse hook to match [`Flags::watching`].
    const WM_WATCH: u32 = WM_APP + 2;

    /// What the hook procedure needs, owned by the hook thread.
    struct HookState {
        machine: Machine,
        vk: u32,
        /// Right Alt is AltGr on many layouts, which sends a fake Left Ctrl with it.
        ignore_lctrl: bool,
        flags: Arc<Flags>,
        thread: u32,
    }

    thread_local! {
        static STATE: RefCell<Option<HookState>> = const { RefCell::new(None) };
    }

    /// The installed push-to-talk hook. Dropping it unhooks.
    pub struct KeyHook {
        thread_id: u32,
        hook_thread: Option<JoinHandle<()>>,
        command_thread: Option<JoinHandle<()>>,
    }

    impl KeyHook {
        pub fn install(key: PushToTalkKey, flags: Arc<Flags>, handler: impl Handler) -> std::io::Result<Self> {
            let (vk, extended) = virtual_key(key);
            let (commands_tx, commands_rx) = mpsc::channel::<Command>();
            let command_thread = std::thread::Builder::new().name("bolo-ptt-commands".into()).spawn(move || {
                for command in commands_rx {
                    match command {
                        Command::ReplayTap => replay(vk, extended),
                        c => handler.command(c),
                    }
                }
            })?;
            let (ready_tx, ready_rx) = mpsc::sync_channel::<std::io::Result<u32>>(1);
            let hook_thread = std::thread::Builder::new()
                .name("bolo-ptt-hook".into())
                .spawn(move || hook_thread(vk, key == PushToTalkKey::RightAlt, flags, commands_tx, ready_tx))?;
            match ready_rx.recv() {
                Ok(Ok(thread_id)) => {
                    Ok(Self { thread_id, hook_thread: Some(hook_thread), command_thread: Some(command_thread) })
                }
                Ok(Err(e)) => Err(e),
                Err(_) => Err(std::io::Error::other("the hook thread exited")),
            }
        }

        /// The hook thread, for [`super::watch_changed`].
        pub fn thread_id(&self) -> u32 {
            self.thread_id
        }
    }

    pub(super) fn post_watch(thread_id: u32) {
        // SAFETY: posting a message; a thread that has exited just makes it fail.
        unsafe { PostThreadMessageW(thread_id, WM_WATCH, 0, 0) };
    }

    impl Drop for KeyHook {
        fn drop(&mut self) {
            // SAFETY: as above.
            unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0) };
            // The hook thread drops the command sender on exit, which ends the command thread.
            for t in [self.hook_thread.take(), self.command_thread.take()].into_iter().flatten() {
                let _ = t.join();
            }
        }
    }

    fn hook_thread(
        vk: u16,
        ignore_lctrl: bool,
        flags: Arc<Flags>,
        commands: mpsc::Sender<Command>,
        ready: mpsc::SyncSender<std::io::Result<u32>>,
    ) {
        // SAFETY: Win32 calls on this thread; `msg` is a valid out-pointer, the hook
        // procedures are `extern "system"` functions that live for the whole program, and
        // every hook is unhooked before the thread ends.
        unsafe {
            let mut msg: MSG = std::mem::zeroed();
            // Create this thread's message queue before anyone posts to it.
            PeekMessageW(&mut msg, null_mut(), 0, 0, PM_NOREMOVE);
            let thread = GetCurrentThreadId();
            STATE.with_borrow_mut(|s| {
                *s = Some(HookState {
                    machine: Machine::default(),
                    vk: u32::from(vk),
                    ignore_lctrl,
                    flags: flags.clone(),
                    thread,
                })
            });
            let module = GetModuleHandleW(std::ptr::null());
            let keyboard = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), module, 0);
            if keyboard.is_null() {
                let _ = ready.send(Err(std::io::Error::last_os_error()));
                return;
            }
            let _ = ready.send(Ok(thread));
            let mut mouse: HHOOK = null_mut();
            'run: loop {
                let deadline = STATE.with_borrow(|s| s.as_ref().and_then(|s| s.machine.deadline()));
                let timeout = deadline.map_or(INFINITE, |d| {
                    d.saturating_duration_since(Instant::now()).as_millis().min(u128::from(INFINITE - 1)) as u32
                });
                MsgWaitForMultipleObjects(0, std::ptr::null(), 0, timeout, QS_ALLINPUT);
                while PeekMessageW(&mut msg, null_mut(), 0, 0, PM_REMOVE) != 0 {
                    match msg.message {
                        WM_QUIT => break 'run,
                        WM_COMMAND => {
                            if let Some(c) = Command::from_code(msg.wParam) {
                                let _ = commands.send(c);
                            }
                        }
                        WM_WATCH => {
                            let watching = flags.watching();
                            if watching && mouse.is_null() {
                                mouse = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), module, 0);
                            } else if !watching && !mouse.is_null() {
                                UnhookWindowsHookEx(mouse);
                                mouse = null_mut();
                            }
                        }
                        _ => {
                            TranslateMessage(&msg);
                            DispatchMessageW(&msg);
                        }
                    }
                }
                let started = STATE.with_borrow_mut(|s| s.as_mut().and_then(|s| s.machine.tick(Instant::now())));
                if let Some(c) = started {
                    let _ = commands.send(c);
                }
            }
            if !mouse.is_null() {
                UnhookWindowsHookEx(mouse);
            }
            UnhookWindowsHookEx(keyboard);
            STATE.with_borrow_mut(|s| *s = None);
        }
    }

    fn held(vk: u16) -> bool {
        // SAFETY: a plain query; the high bit is "down now".
        unsafe { GetAsyncKeyState(i32::from(vk)) < 0 }
    }

    /// Shift, Ctrl, Alt or Win is down, not counting the trigger itself.
    fn modifiers(trigger: u32, ignore_lctrl: bool) -> bool {
        [VK_LSHIFT, VK_RSHIFT, VK_LCONTROL, VK_RCONTROL, VK_LMENU, VK_RMENU, VK_LWIN, VK_RWIN]
            .into_iter()
            .filter(|&vk| u32::from(vk) != trigger && !(ignore_lctrl && vk == VK_LCONTROL))
            .any(held)
    }

    /// The hook procedure. It must return fast: no lock, no allocation, no blocking.
    unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0 {
            // SAFETY: for WH_KEYBOARD_LL with code >= 0, lparam points at a KBDLLHOOKSTRUCT.
            let info = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
            let down = matches!(wparam as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
            let ours = info.flags & LLKHF_INJECTED != 0 && info.dwExtraInfo == BOLO_TAG;
            let swallow = STATE.with(|cell| {
                // Never re-entered on this thread; `try_borrow_mut` is only a guard.
                let Ok(mut state) = cell.try_borrow_mut() else { return false };
                let Some(s) = state.as_mut() else { return false };
                if down && !ours {
                    s.flags.touch();
                }
                let key = if info.vkCode == s.vk {
                    Key::Trigger
                } else if info.vkCode == u32::from(VK_ESCAPE) {
                    Key::Esc
                } else {
                    return false;
                };
                let modifiers = key == Key::Trigger && down && modifiers(s.vk, s.ignore_lctrl);
                let ev = KeyEvent { key, down, modifiers, injected: ours };
                let out = s.machine.key(ev, Instant::now(), s.flags.listening());
                for c in out.commands() {
                    // SAFETY: posting to our own thread's queue.
                    unsafe { PostThreadMessageW(s.thread, WM_COMMAND, c.to_code(), 0) };
                }
                out.disposition == Disposition::Swallow
            });
            if swallow {
                return 1;
            }
        }
        // SAFETY: passing the event on, as the hook contract requires.
        unsafe { CallNextHookEx(null_mut(), code, wparam, lparam) }
    }

    /// Installed only while a polish swap is watched: any click counts as a touch.
    unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0 && matches!(wparam as u32, WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_XBUTTONDOWN) {
            STATE.with(|cell| {
                if let Ok(state) = cell.try_borrow()
                    && let Some(s) = state.as_ref()
                {
                    s.flags.touch();
                }
            });
        }
        // SAFETY: passing the event on.
        unsafe { CallNextHookEx(null_mut(), code, wparam, lparam) }
    }

    /// Press and release the trigger, tagged so the hook lets it through.
    fn replay(vk: u16, extended: bool) {
        let flags = if extended { KEYEVENTF_EXTENDEDKEY } else { 0 };
        let input = |up: bool| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: flags | if up { KEYEVENTF_KEYUP } else { 0 },
                    time: 0,
                    dwExtraInfo: BOLO_TAG,
                },
            },
        };
        let inputs = [input(false), input(true)];
        // SAFETY: a valid array of INPUTs and its length and element size.
        let sent = unsafe { SendInput(inputs.len() as u32, inputs.as_ptr(), size_of::<INPUT>() as i32) };
        if sent as usize != inputs.len() {
            tracing::warn!(sent, "the tap replay was blocked");
        }
    }
}

#[cfg(test)]
mod tests {
    use bolo_store::PushToTalkKey;

    use super::virtual_key;

    #[test]
    fn every_key_maps_to_its_virtual_key() {
        assert_eq!(virtual_key(PushToTalkKey::CapsLock), (0x14, false));
        assert_eq!(virtual_key(PushToTalkKey::RightCtrl), (0xA3, true));
        assert_eq!(virtual_key(PushToTalkKey::RightAlt), (0xA5, true));
        assert_eq!(virtual_key(PushToTalkKey::F13), (0x7C, false));
        assert_eq!(virtual_key(PushToTalkKey::F24), (0x87, false));
    }
}
