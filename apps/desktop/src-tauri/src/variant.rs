//! Which Bolo this is (ADR-0019): **Bolo**, the releases, or **Bolo Nightly**, a build of
//! every push to `main`. They are separate apps, like a browser's stable and canary
//! builds:
//! - each has its own name, icon, install folder, Start menu entry and update feed;
//! - they share the settings, models, history and API keys;
//! - only one holds the push-to-talk key at a time. Starting or resuming one asks the
//!   other to pause ([`ask_to_step_aside`], [`listen_for_step_aside`]).

use serde::Serialize;

/// This build's version; a nightly's carries `-nightly.<date>.<run>`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

const REPO: &str = "https://github.com/Nirav1810/bolo";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Variant {
    Release,
    Nightly,
}

impl Variant {
    pub fn of(version: &str) -> Self {
        if version.contains("-nightly.") { Variant::Nightly } else { Variant::Release }
    }

    /// The running build's variant.
    pub fn current() -> Self {
        Self::of(VERSION)
    }

    pub fn other(self) -> Self {
        match self {
            Variant::Release => Variant::Nightly,
            Variant::Nightly => Variant::Release,
        }
    }

    /// The app's name, as in `tauri.conf.json` and `tauri.nightly.conf.json`.
    pub fn product_name(self) -> &'static str {
        match self {
            Variant::Release => "Bolo",
            Variant::Nightly => "Bolo Nightly",
        }
    }

    /// The updater feed. Releases are GitHub's "latest" (not prereleases); nightlies are
    /// the rolling `nightly` release.
    pub fn feed(self) -> String {
        match self {
            Variant::Release => format!("{REPO}/releases/latest/download/latest.json"),
            Variant::Nightly => format!("{REPO}/releases/download/nightly/latest.json"),
        }
    }

    /// Where to download this variant by hand.
    pub fn download_page(self) -> String {
        match self {
            Variant::Release => format!("{REPO}/releases/latest"),
            Variant::Nightly => format!("{REPO}/releases/tag/nightly"),
        }
    }

    /// The log file in `%LOCALAPPDATA%\Bolo\data\logs`, one per variant.
    pub fn log_file(self) -> &'static str {
        match self {
            Variant::Release => "desktop.log",
            Variant::Nightly => "desktop-nightly.log",
        }
    }

    /// The named event the other variant sets to ask this one to pause.
    fn step_aside_event(self) -> &'static str {
        match self {
            Variant::Release => "Local\\dev.bolo.desktop.step-aside",
            Variant::Nightly => "Local\\dev.bolo.desktop.nightly.step-aside",
        }
    }
}

/// The label after the version: Nightly, or Beta for every 0.x release, none from 1.0 on.
pub fn badge(version: &str) -> Option<&'static str> {
    match Variant::of(version) {
        Variant::Nightly => Some("Nightly"),
        Variant::Release if version.starts_with("0.") => Some("Beta"),
        Variant::Release => None,
    }
}

/// `Bolo 0.4.0 · Beta` or `Bolo Nightly 0.5.0-nightly.20261112.3`: the tray tooltip and
/// the first line of its menu.
pub fn title(version: &str) -> String {
    let variant = Variant::of(version);
    match (variant, badge(version)) {
        (Variant::Release, Some(b)) => format!("Bolo {version} · {b}"),
        _ => format!("{} {version}", variant.product_name()),
    }
}

/// Ask the other variant, if it runs, to pause and let go of the push-to-talk key.
/// Returns whether it was running.
pub fn ask_to_step_aside(other: Variant) -> bool {
    #[cfg(windows)]
    {
        win::signal(other.step_aside_event())
    }
    #[cfg(not(windows))]
    {
        let _ = other;
        false
    }
}

/// Run `on_request` each time the other variant asks this one to step aside.
pub fn listen_for_step_aside(me: Variant, on_request: impl Fn() + Send + 'static) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        win::listen(me.step_aside_event(), on_request)
    }
    #[cfg(not(windows))]
    {
        let _ = (me, on_request);
        Ok(())
    }
}

#[cfg(windows)]
mod win {
    use std::io;

    use windows_sys::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Threading::{
        CreateEventW, EVENT_MODIFY_STATE, INFINITE, OpenEventW, SetEvent, WaitForSingleObject,
    };

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    pub fn signal(name: &str) -> bool {
        let name = wide(name);
        // SAFETY: `name` is a NUL-terminated UTF-16 string that outlives the call; the
        // handle is closed once set.
        unsafe {
            let event = OpenEventW(EVENT_MODIFY_STATE, 0, name.as_ptr());
            if event.is_null() {
                return false;
            }
            SetEvent(event);
            CloseHandle(event);
        }
        true
    }

    pub fn listen(name: &str, on_request: impl Fn() + Send + 'static) -> io::Result<()> {
        let name = wide(name);
        // SAFETY: as above. Auto-reset, initially unset; the handle lives as long as the
        // process (the thread below waits on it forever).
        let event = unsafe { CreateEventW(std::ptr::null(), 0, 0, name.as_ptr()) };
        if event.is_null() {
            return Err(io::Error::last_os_error());
        }
        let handle = event as usize;
        std::thread::Builder::new().name("bolo-step-aside".into()).spawn(move || {
            loop {
                // SAFETY: the handle is never closed while this thread runs.
                let woke = unsafe { WaitForSingleObject(handle as _, INFINITE) };
                if woke != WAIT_OBJECT_0 {
                    tracing::warn!(woke, "stopped listening for the other Bolo");
                    return;
                }
                on_request();
            }
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_version_says_which_app_this_is() {
        assert_eq!(Variant::of("0.4.0"), Variant::Release);
        assert_eq!(Variant::of("0.5.0-nightly.20261111.7"), Variant::Nightly);
        assert_eq!(Variant::Nightly.other(), Variant::Release);
        assert_eq!(Variant::Nightly.product_name(), "Bolo Nightly");
        assert!(Variant::Nightly.feed().ends_with("/releases/download/nightly/latest.json"));
        assert!(Variant::Release.feed().ends_with("/releases/latest/download/latest.json"));
        assert_ne!(Variant::Release.log_file(), Variant::Nightly.log_file());
    }

    #[test]
    fn badges_and_titles_name_the_variant() {
        assert_eq!(badge("0.4.0"), Some("Beta"));
        assert_eq!(badge("0.5.0-nightly.20261111.7"), Some("Nightly"));
        assert_eq!(badge("1.0.0"), None);
        assert_eq!(title("0.4.0"), "Bolo 0.4.0 · Beta");
        assert_eq!(title("1.0.0"), "Bolo 1.0.0");
        assert_eq!(title("0.5.0-nightly.20261111.7"), "Bolo Nightly 0.5.0-nightly.20261111.7");
    }

    #[cfg(windows)]
    #[test]
    fn asking_the_other_variant_reaches_its_listener() {
        use std::sync::mpsc;
        use std::time::Duration;

        // A test-only name, so a running Bolo on this machine isn't asked.
        let name = format!("Local\\dev.bolo.desktop.test-{}", std::process::id());
        assert!(!win::signal(&name), "nobody listens yet");
        let (tx, rx) = mpsc::channel();
        win::listen(&name, move || tx.send(()).unwrap()).unwrap();
        assert!(win::signal(&name));
        rx.recv_timeout(Duration::from_secs(5)).expect("the listener was asked");
        assert!(win::signal(&name));
        rx.recv_timeout(Duration::from_secs(5)).expect("and again");
    }
}
