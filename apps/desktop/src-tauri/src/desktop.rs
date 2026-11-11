//! The seam between the app's logic and the Windows desktop: the window in front, the
//! clipboard, and synthesized keys.
//!
//! Everything that decides *what* to do (the controller, insertion, the polish swap)
//! talks to a [`Desktop`], so tests drive it with [`fake::FakeDesktop`], which records the
//! clipboard reads and writes and the keys sent. [`WinDesktop`] is the real one.

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

/// The clipboard formats Bolo saves and restores around a paste (ADR-0018). Anything
/// else on the clipboard (an Excel range's private formats, say) is lost by a dictation.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Clipboard {
    /// `CF_UNICODETEXT`.
    pub text: Option<String>,
    /// `CF_HTML` as it sits on the clipboard: the header with its byte offsets, then the
    /// HTML. Built with [`crate::insert::cf_html`].
    pub html: Option<String>,
    /// `CF_DIBV5`, as raw bytes.
    pub image: Option<Vec<u8>>,
    /// `CF_HDROP` (a `DROPFILES` block), as raw bytes.
    pub files: Option<Vec<u8>>,
}

impl Clipboard {
    pub fn is_empty(&self) -> bool {
        self.text.is_none() && self.html.is_none() && self.image.is_none() && self.files.is_none()
    }
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
    #[error("{0} needs Windows")]
    Unsupported(&'static str),
    #[error("the clipboard is unavailable: {0}")]
    Clipboard(String),
    /// The keys can't reach the window in front, because it runs elevated (UIPI).
    #[error("keys were blocked by the target window")]
    Blocked,
    /// `SendInput` sent only some of the keys, for another reason.
    #[error("the keys could not be sent: {0}")]
    Keys(String),
}

pub trait Desktop: Send + Sync {
    /// The window in front, if there is one.
    fn foreground(&self) -> Option<Foreground>;
    fn read_clipboard(&self) -> Result<Clipboard, DesktopError>;
    /// Replace the clipboard with `content`. The write is kept out of clipboard history
    /// (Win+V) and clipboard monitors. Returns the clipboard sequence number after it.
    fn write_clipboard(&self, content: &Clipboard) -> Result<u32, DesktopError>;
    /// Bumped by Windows on every clipboard change, ours or anyone's.
    fn clipboard_sequence(&self) -> u32;
    /// Send the keys to the window in front, tagged with [`crate::keyhook::BOLO_TAG`] so
    /// the keyboard hook ignores them. [`DesktopError::Blocked`] if that window runs at a
    /// higher integrity level than Bolo (UIPI drops the keys silently).
    fn send_keys(&self, keys: &[Key]) -> Result<(), DesktopError>;
}

impl fmt::Display for Foreground {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.exe)
    }
}

/// The real desktop.
#[derive(Debug, Default)]
pub struct WinDesktop;

impl Desktop for WinDesktop {
    fn foreground(&self) -> Option<Foreground> {
        win::foreground()
    }
    fn read_clipboard(&self) -> Result<Clipboard, DesktopError> {
        win::read_clipboard()
    }
    fn write_clipboard(&self, content: &Clipboard) -> Result<u32, DesktopError> {
        win::write_clipboard(content)
    }
    fn clipboard_sequence(&self) -> u32 {
        win::clipboard_sequence()
    }
    fn send_keys(&self, keys: &[Key]) -> Result<(), DesktopError> {
        win::send_keys(keys)
    }
}

#[cfg(windows)]
mod win {
    use std::ptr::null_mut;
    use std::time::Duration;

    use windows_sys::Win32::Foundation::{CloseHandle, GlobalFree, HANDLE, MAX_PATH};
    use windows_sys::Win32::Security::{
        GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, TOKEN_MANDATORY_LABEL, TOKEN_QUERY,
        TokenIntegrityLevel,
    };
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardSequenceNumber, IsClipboardFormatAvailable,
        OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
    };
    use windows_sys::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock};
    use windows_sys::Win32::System::Ole::{CF_DIBV5, CF_HDROP, CF_UNICODETEXT};
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        QueryFullProcessImageNameW,
    };
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, MAPVK_VK_TO_VSC,
        MapVirtualKeyW, SendInput, VK_CONTROL, VK_INSERT, VK_LEFT, VK_SHIFT,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId};

    use super::{Clipboard, DesktopError, Foreground, Key};
    use crate::keyhook::BOLO_TAG;

    /// `SECURITY_MANDATORY_HIGH_RID`: the integrity level of an elevated process.
    const HIGH_INTEGRITY: u32 = 0x3000;
    const VK_V: u16 = 0x56;

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

    fn registered(name: &str) -> u32 {
        let wide: Vec<u16> = name.encode_utf16().chain([0]).collect();
        // SAFETY: a NUL-terminated UTF-16 string that outlives the call.
        unsafe { RegisterClipboardFormatW(wide.as_ptr()) }
    }

    fn os_error() -> DesktopError {
        DesktopError::Clipboard(std::io::Error::last_os_error().to_string())
    }

    /// The clipboard, opened; closed again on drop. Another app may hold it for a moment,
    /// so opening retries for up to ~100 ms.
    /// Not `Send`: the clipboard is opened by, and must be closed on, one thread.
    struct Open(std::marker::PhantomData<*const ()>);

    impl Open {
        fn new() -> Result<Self, DesktopError> {
            for _ in 0..20 {
                // SAFETY: no owner window; paired with CloseClipboard in Drop.
                if unsafe { OpenClipboard(null_mut()) } != 0 {
                    return Ok(Open(std::marker::PhantomData));
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(os_error())
        }
    }

    impl Drop for Open {
        fn drop(&mut self) {
            // SAFETY: the clipboard is open on this thread.
            unsafe { CloseClipboard() };
        }
    }

    /// One format's bytes, with the clipboard open.
    fn get(_: &Open, format: u32) -> Option<Vec<u8>> {
        // SAFETY: the clipboard is open (the `Open` borrow); the handle is the system's and
        // is only locked for the copy, which reads exactly `GlobalSize` bytes.
        unsafe {
            if IsClipboardFormatAvailable(format) == 0 {
                return None;
            }
            let handle = GetClipboardData(format);
            if handle.is_null() {
                return None;
            }
            let size = GlobalSize(handle);
            let ptr = GlobalLock(handle) as *const u8;
            if ptr.is_null() {
                return None;
            }
            let bytes = std::slice::from_raw_parts(ptr, size).to_vec();
            GlobalUnlock(handle);
            Some(bytes)
        }
    }

    /// Put one format on the clipboard, with the clipboard open and emptied.
    fn set(_: &Open, format: u32, bytes: &[u8]) -> Result<(), DesktopError> {
        // SAFETY: a fresh movable block at least `bytes.len()` long, locked only for the
        // copy; on success the system owns it, on failure we free it.
        unsafe {
            let handle = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1));
            if handle.is_null() {
                return Err(os_error());
            }
            let ptr = GlobalLock(handle) as *mut u8;
            if ptr.is_null() {
                GlobalFree(handle);
                return Err(os_error());
            }
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
            GlobalUnlock(handle);
            if SetClipboardData(format, handle as HANDLE).is_null() {
                let e = os_error();
                GlobalFree(handle);
                return Err(e);
            }
            Ok(())
        }
    }

    /// Bytes up to the first NUL.
    fn until_nul<T: PartialEq + Default + Copy>(v: &[T]) -> &[T] {
        let end = v.iter().position(|&c| c == T::default()).unwrap_or(v.len());
        &v[..end]
    }

    pub fn read_clipboard() -> Result<Clipboard, DesktopError> {
        let html_format = registered("HTML Format");
        let open = Open::new()?;
        let text = get(&open, u32::from(CF_UNICODETEXT)).map(|b| {
            let wide: Vec<u16> = b.as_chunks::<2>().0.iter().map(|&c| u16::from_le_bytes(c)).collect();
            String::from_utf16_lossy(until_nul(&wide))
        });
        let html = get(&open, html_format).map(|b| String::from_utf8_lossy(until_nul(&b)).into_owned());
        Ok(Clipboard { text, html, image: get(&open, u32::from(CF_DIBV5)), files: get(&open, u32::from(CF_HDROP)) })
    }

    pub fn write_clipboard(content: &Clipboard) -> Result<u32, DesktopError> {
        let html_format = registered("HTML Format");
        // Keep Bolo's writes (dictations, and restoring what the user had) out of Win+V
        // history, cloud clipboard and clipboard managers.
        let private = [
            registered("ExcludeClipboardContentFromMonitorProcessing"),
            registered("CanIncludeInClipboardHistory"),
            registered("CanUploadToCloudClipboard"),
        ];
        let open = Open::new()?;
        // SAFETY: the clipboard is open on this thread.
        if unsafe { EmptyClipboard() } == 0 {
            return Err(os_error());
        }
        if !content.is_empty() {
            if let Some(text) = &content.text {
                let bytes: Vec<u8> = text.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect();
                set(&open, u32::from(CF_UNICODETEXT), &bytes)?;
            }
            if let Some(html) = &content.html {
                set(&open, html_format, &[html.as_bytes(), &[0]].concat())?;
            }
            if let Some(image) = &content.image {
                set(&open, u32::from(CF_DIBV5), image)?;
            }
            if let Some(files) = &content.files {
                set(&open, u32::from(CF_HDROP), files)?;
            }
            for format in private {
                set(&open, format, &0u32.to_le_bytes())?;
            }
        }
        // Windows bumps the sequence number when the clipboard closes, so it's read after.
        // Another app writing in the microseconds between would pass for Bolo's write.
        drop(open);
        Ok(clipboard_sequence())
    }

    pub fn clipboard_sequence() -> u32 {
        // SAFETY: a plain query.
        unsafe { GetClipboardSequenceNumber() }
    }

    /// The integrity level (a `SECURITY_MANDATORY_*_RID`) of a process, or `None` if its
    /// token can't be read, which for another user's or an elevated process means it's
    /// above us.
    fn integrity(process: HANDLE) -> Option<u32> {
        // SAFETY: the token handle is closed; the buffer is u64-aligned and large enough
        // for a TOKEN_MANDATORY_LABEL and its SID, and is only read after the call fills it.
        unsafe {
            let mut token: HANDLE = null_mut();
            if OpenProcessToken(process, TOKEN_QUERY, &mut token) == 0 {
                return None;
            }
            let mut buf = [0u64; 16];
            let mut len = 0u32;
            let ok = GetTokenInformation(
                token,
                TokenIntegrityLevel,
                buf.as_mut_ptr().cast(),
                size_of_val(&buf) as u32,
                &mut len,
            );
            CloseHandle(token);
            if ok == 0 {
                return None;
            }
            let label = &*(buf.as_ptr() as *const TOKEN_MANDATORY_LABEL);
            let sid = label.Label.Sid;
            let count = u32::from(*GetSidSubAuthorityCount(sid));
            Some(*GetSidSubAuthority(sid, count.checked_sub(1)?))
        }
    }

    /// The window in front runs at a higher integrity level than Bolo, so UIPI drops the
    /// keys Bolo sends it (and `SendInput` doesn't say so).
    fn foreground_blocked() -> bool {
        // SAFETY: plain queries; the process handle is closed.
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.is_null() {
                return false;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, &mut pid);
            let Some(ours) = integrity(GetCurrentProcess()) else { return false };
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if process.is_null() {
                return ours < HIGH_INTEGRITY;
            }
            let theirs = integrity(process);
            CloseHandle(process);
            match theirs {
                Some(theirs) => theirs > ours,
                None => ours < HIGH_INTEGRITY,
            }
        }
    }

    fn key_input(vk: u16, up: bool, extended: bool) -> INPUT {
        let mut flags = if up { KEYEVENTF_KEYUP } else { 0 };
        if extended {
            flags |= KEYEVENTF_EXTENDEDKEY;
        }
        // SAFETY: a plain lookup.
        let scan = unsafe { MapVirtualKeyW(u32::from(vk), MAPVK_VK_TO_VSC) } as u16;
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT { wVk: vk, wScan: scan, dwFlags: flags, time: 0, dwExtraInfo: BOLO_TAG },
            },
        }
    }

    pub fn send_keys(keys: &[Key]) -> Result<(), DesktopError> {
        if foreground_blocked() {
            return Err(DesktopError::Blocked);
        }
        let mut inputs = Vec::with_capacity(keys.len() * 4);
        for key in keys {
            let (modifier, vk, extended) = match key {
                Key::Paste => (VK_CONTROL, VK_V, false),
                Key::ShiftInsert => (VK_SHIFT, VK_INSERT, true),
                Key::ShiftLeft => (VK_SHIFT, VK_LEFT, true),
            };
            inputs.extend([
                key_input(modifier, false, false),
                key_input(vk, false, extended),
                key_input(vk, true, extended),
                key_input(modifier, true, false),
            ]);
        }
        // SAFETY: a valid array of INPUTs, its length and the element size.
        let sent = unsafe { SendInput(inputs.len() as u32, inputs.as_ptr(), size_of::<INPUT>() as i32) };
        if sent as usize != inputs.len() {
            let e = std::io::Error::last_os_error();
            // Don't leave a modifier held down by a half-sent chord.
            let release = [key_input(VK_CONTROL, true, false), key_input(VK_SHIFT, true, false)];
            // SAFETY: as above.
            unsafe { SendInput(release.len() as u32, release.as_ptr(), size_of::<INPUT>() as i32) };
            return Err(DesktopError::Keys(format!("{sent} of {} key events sent: {e}", inputs.len())));
        }
        Ok(())
    }
}

#[cfg(not(windows))]
mod win {
    use super::{Clipboard, DesktopError, Foreground, Key};

    pub fn foreground() -> Option<Foreground> {
        None
    }
    pub fn read_clipboard() -> Result<Clipboard, DesktopError> {
        Err(DesktopError::Unsupported("the clipboard"))
    }
    pub fn write_clipboard(_: &Clipboard) -> Result<u32, DesktopError> {
        Err(DesktopError::Unsupported("the clipboard"))
    }
    pub fn clipboard_sequence() -> u32 {
        0
    }
    pub fn send_keys(_: &[Key]) -> Result<(), DesktopError> {
        Err(DesktopError::Unsupported("sending keys"))
    }
}

/// A desktop for tests: a scripted window in front, a clipboard in memory with a
/// sequence number, and a log of every clipboard read and write and every key sent.
#[cfg(test)]
pub mod fake {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::{Clipboard, Desktop, DesktopError, Foreground, Key};

    /// One thing done to the fake desktop, in order.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Op {
        Read,
        Write(Clipboard),
        Keys(Vec<Key>),
    }

    #[derive(Debug, Default)]
    pub struct FakeDesktop {
        pub foreground: Mutex<Option<Foreground>>,
        pub clipboard: Mutex<Clipboard>,
        pub sequence: Mutex<u32>,
        pub ops: Mutex<Vec<Op>>,
        /// The window in front runs elevated: keys are refused.
        pub blocked: AtomicBool,
    }

    impl FakeDesktop {
        pub fn in_front(exe: &str, title: &str) -> Self {
            let fake = Self::default();
            fake.focus(exe, title, 1);
            fake
        }

        /// Put another window in front.
        pub fn focus(&self, exe: &str, title: &str, window: isize) {
            *self.foreground.lock().unwrap() = Some(Foreground { exe: exe.into(), title: title.into(), window });
        }

        /// The user copies some text (not logged as an op).
        pub fn user_copies(&self, text: &str) {
            *self.clipboard.lock().unwrap() = Clipboard { text: Some(text.into()), ..Default::default() };
            *self.sequence.lock().unwrap() += 1;
        }

        pub fn text(&self) -> Option<String> {
            self.clipboard.lock().unwrap().text.clone()
        }

        pub fn take_ops(&self) -> Vec<Op> {
            std::mem::take(&mut *self.ops.lock().unwrap())
        }

        pub fn set_blocked(&self, blocked: bool) {
            self.blocked.store(blocked, Ordering::Relaxed);
        }
    }

    impl Desktop for FakeDesktop {
        fn foreground(&self) -> Option<Foreground> {
            self.foreground.lock().unwrap().clone()
        }
        fn read_clipboard(&self) -> Result<Clipboard, DesktopError> {
            self.ops.lock().unwrap().push(Op::Read);
            Ok(self.clipboard.lock().unwrap().clone())
        }
        fn write_clipboard(&self, content: &Clipboard) -> Result<u32, DesktopError> {
            *self.clipboard.lock().unwrap() = content.clone();
            self.ops.lock().unwrap().push(Op::Write(content.clone()));
            let mut seq = self.sequence.lock().unwrap();
            *seq += 1;
            Ok(*seq)
        }
        fn clipboard_sequence(&self) -> u32 {
            *self.sequence.lock().unwrap()
        }
        fn send_keys(&self, keys: &[Key]) -> Result<(), DesktopError> {
            if self.blocked.load(Ordering::Relaxed) {
                return Err(DesktopError::Blocked);
            }
            self.ops.lock().unwrap().push(Op::Keys(keys.to_vec()));
            Ok(())
        }
    }
}

/// Touches the real clipboard, so it's opt-in: `cargo test -p bolo-desktop -- --ignored`.
#[cfg(all(test, windows))]
mod live {
    use super::{Clipboard, Desktop, WinDesktop};
    use crate::insert::cf_html;

    #[test]
    #[ignore = "uses the real clipboard"]
    fn the_clipboard_round_trips_and_bumps_the_sequence() {
        let d = WinDesktop;
        let before = d.read_clipboard().unwrap();
        let ours = Clipboard {
            text: Some("Bolo live test: चाय\r\nline two".into()),
            html: Some(cf_html("<p>चाय</p>")),
            ..Default::default()
        };
        let seq = d.write_clipboard(&ours).unwrap();
        assert_eq!(d.clipboard_sequence(), seq);
        assert_eq!(d.read_clipboard().unwrap(), ours);
        let empty = d.write_clipboard(&Clipboard::default()).unwrap();
        assert!(empty > seq);
        assert!(d.read_clipboard().unwrap().is_empty());
        d.write_clipboard(&before).unwrap();
        assert_eq!(d.read_clipboard().unwrap(), before, "what the user had comes back");
    }

    /// Pastes, swaps in polish and restores the clipboard in an EDIT control it opens.
    /// `SendInput` only reaches the interactive desktop, so run it from a visible console.
    #[test]
    #[ignore = "types into a window it opens; needs the interactive desktop"]
    fn paste_and_swap_into_an_edit_control() {
        use std::ptr::{null, null_mut};
        use std::sync::Arc;
        use std::time::{Duration, Instant};

        use bolo_core::{FormattedOutput, LanguageMode, Timings};
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, DispatchMessageW, ES_MULTILINE, GetForegroundWindow, GetWindowTextW, MSG,
            PM_REMOVE, PeekMessageW, SetForegroundWindow, TranslateMessage, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
        };

        use crate::hotkey::Flags;
        use crate::insert::{Inserted, Inserter, Swapped, Target, ThreadTimer};

        fn pump(for_: Duration) {
            let end = Instant::now() + for_;
            // SAFETY: a message loop on this thread with a valid out-pointer.
            unsafe {
                let mut msg: MSG = std::mem::zeroed();
                while Instant::now() < end {
                    while PeekMessageW(&mut msg, null_mut(), 0, 0, PM_REMOVE) != 0 {
                        TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
        }
        let output = |plain: &str| FormattedOutput {
            plain: plain.into(),
            markdown: plain.into(),
            html: None,
            timings: Timings::default(),
            engine_id: "test".into(),
            language: LanguageMode::English,
        };
        let text_of = |hwnd| {
            let mut buf = [0u16; 512];
            // SAFETY: a sized buffer for a window we own.
            let n = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
            String::from_utf16_lossy(&buf[..n.max(0) as usize])
        };

        let d = Arc::new(WinDesktop);
        let users = d.read_clipboard().unwrap();
        d.write_clipboard(&Clipboard { text: Some("sentinel".into()), ..Default::default() }).unwrap();
        let class: Vec<u16> = "EDIT ".encode_utf16().collect();
        // SAFETY: a top-level EDIT control owned by this thread, destroyed below.
        let hwnd = unsafe {
            let hwnd = CreateWindowExW(
                0,
                class.as_ptr(),
                null(),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE | ES_MULTILINE as u32,
                200,
                200,
                700,
                200,
                null_mut(),
                null_mut(),
                null_mut(),
                null(),
            );
            assert!(!hwnd.is_null());
            SetForegroundWindow(hwnd);
            hwnd
        };
        pump(Duration::from_millis(300));
        // SAFETY: a plain query.
        assert_eq!(unsafe { GetForegroundWindow() }, hwnd, "the test window must be in front");

        let inserter = Inserter::new(d.clone(), Arc::new(ThreadTimer), Arc::new(Flags::default()), || {});
        inserter.begin(Target::default(), true);
        assert_eq!(inserter.insert(&output("helo wrld नमस्ते"), Instant::now()), Inserted::Pasted);
        pump(Duration::from_millis(500));
        let pasted = text_of(hwnd);
        let restored = d.read_clipboard().unwrap().text;
        assert_eq!(inserter.polished(&output("Hello, world. नमस्ते!"), Instant::now()), Swapped::Swapped);
        pump(Duration::from_millis(500));
        let swapped = text_of(hwnd);
        let restored_again = d.read_clipboard().unwrap().text;
        // SAFETY: our own window.
        unsafe { DestroyWindow(hwnd) };
        d.write_clipboard(&users).unwrap();

        assert_eq!(pasted, "helo wrld नमस्ते");
        assert_eq!(restored.as_deref(), Some("sentinel"));
        assert_eq!(swapped, "Hello, world. नमस्ते!");
        assert_eq!(restored_again.as_deref(), Some("sentinel"));
    }
}
