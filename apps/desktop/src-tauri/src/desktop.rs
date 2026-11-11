//! The seam between the app's logic and the Windows desktop: the window in front, the
//! clipboard, and synthesized keys.
//!
//! Everything that decides *what* to do (the controller, insertion, the polish swap)
//! talks to a [`Desktop`], so tests drive it with [`fake::FakeDesktop`], which records the
//! clipboard writes and the keys sent. [`WinDesktop`] is the real one.

use std::fmt;

/// The app in front, which the target context is chosen from (M3 ticket 07).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Foreground {
    /// The process image name, lower-case, e.g. `notepad.exe`.
    pub exe: String,
    /// The window title. Browsers are told apart by it (Gmail in Chrome).
    pub title: String,
    /// The window handle, to check the same window is still in front.
    pub window: isize,
}

/// What Bolo reads from and puts on the clipboard. Ticket 06 (ADR-0018) adds the image
/// and file-list formats it saves and restores.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Clipboard {
    pub text: Option<String>,
    /// The HTML fragment (`CF_HTML` without its header), for HTML targets.
    pub html: Option<String>,
}

/// The key strokes Bolo synthesizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// Ctrl+V: the default paste chord.
    Paste,
    /// Shift+Insert: the paste chord for apps where Ctrl+V means something else.
    ShiftInsert,
    /// Shift+Left: select one caret unit back (the polish swap).
    ShiftLeft,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DesktopError {
    #[error("{0} is not implemented yet")]
    Unsupported(&'static str),
    #[error("the clipboard is unavailable: {0}")]
    Clipboard(String),
    /// `SendInput` was blocked, typically because the target runs elevated (UIPI).
    #[error("keys were blocked by the target window")]
    Blocked,
}

pub trait Desktop: Send + Sync {
    /// The window in front, if there is one.
    fn foreground(&self) -> Option<Foreground>;
    fn read_clipboard(&self) -> Result<Clipboard, DesktopError>;
    fn write_clipboard(&self, content: &Clipboard) -> Result<(), DesktopError>;
    fn send_keys(&self, keys: &[Key]) -> Result<(), DesktopError>;
}

impl fmt::Display for Foreground {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.exe)
    }
}

/// The real desktop. The window in front works now; the clipboard and keys arrive with
/// insertion (ticket 06).
#[derive(Debug, Default)]
pub struct WinDesktop;

impl Desktop for WinDesktop {
    fn foreground(&self) -> Option<Foreground> {
        win::foreground()
    }
    fn read_clipboard(&self) -> Result<Clipboard, DesktopError> {
        Err(DesktopError::Unsupported("reading the clipboard"))
    }
    fn write_clipboard(&self, _: &Clipboard) -> Result<(), DesktopError> {
        Err(DesktopError::Unsupported("writing the clipboard"))
    }
    fn send_keys(&self, _: &[Key]) -> Result<(), DesktopError> {
        Err(DesktopError::Unsupported("sending keys"))
    }
}

#[cfg(windows)]
mod win {
    use windows_sys::Win32::Foundation::{CloseHandle, MAX_PATH};
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId};

    use super::Foreground;

    pub fn foreground() -> Option<Foreground> {
        // SAFETY: plain Win32 queries on handles we own or that the system hands us; every
        // buffer is sized and its length passed in, and the process handle is closed.
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.is_null() {
                return None;
            }
            let mut title = [0u16; 512];
            let n = GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as i32).max(0) as usize;
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, &mut pid);
            let mut exe = String::new();
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if !process.is_null() {
                let mut path = [0u16; MAX_PATH as usize * 2];
                let mut len = path.len() as u32;
                if QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, path.as_mut_ptr(), &mut len) != 0 {
                    let path = String::from_utf16_lossy(&path[..len as usize]);
                    exe = path.rsplit(['\\', '/']).next().unwrap_or_default().to_lowercase();
                }
                CloseHandle(process);
            }
            Some(Foreground { exe, title: String::from_utf16_lossy(&title[..n]), window: hwnd as isize })
        }
    }
}

#[cfg(not(windows))]
mod win {
    pub fn foreground() -> Option<super::Foreground> {
        None
    }
}

/// A desktop for tests: a scripted window in front, a clipboard in memory, and a log of
/// every key sent.
#[cfg(test)]
pub mod fake {
    use std::sync::Mutex;

    use super::{Clipboard, Desktop, DesktopError, Foreground, Key};

    #[derive(Debug, Default)]
    pub struct FakeDesktop {
        pub foreground: Mutex<Option<Foreground>>,
        pub clipboard: Mutex<Clipboard>,
        /// Every clipboard write, in order.
        pub writes: Mutex<Vec<Clipboard>>,
        pub keys: Mutex<Vec<Key>>,
    }

    impl FakeDesktop {
        pub fn in_front(exe: &str, title: &str) -> Self {
            let fake = Self::default();
            *fake.foreground.lock().unwrap() = Some(Foreground { exe: exe.into(), title: title.into(), window: 1 });
            fake
        }
    }

    impl Desktop for FakeDesktop {
        fn foreground(&self) -> Option<Foreground> {
            self.foreground.lock().unwrap().clone()
        }
        fn read_clipboard(&self) -> Result<Clipboard, DesktopError> {
            Ok(self.clipboard.lock().unwrap().clone())
        }
        fn write_clipboard(&self, content: &Clipboard) -> Result<(), DesktopError> {
            *self.clipboard.lock().unwrap() = content.clone();
            self.writes.lock().unwrap().push(content.clone());
            Ok(())
        }
        fn send_keys(&self, keys: &[Key]) -> Result<(), DesktopError> {
            self.keys.lock().unwrap().extend_from_slice(keys);
            Ok(())
        }
    }
}
