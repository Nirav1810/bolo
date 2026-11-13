//! Which Bolo this is (ADR-0019). There is one app, with two **channels**: the releases
//! (Beta until 1.0) and Nightly, a build of every push to `main`. The build that runs
//! decides the name, icon and pill colour; picking the other channel in About reinstalls
//! Bolo from that channel's feed ([`crate::updates`]), so they all change together.
//!
//! Bolo Nightly used to be a separate app installed next to Bolo. [`retire_old_nightly`]
//! removes that install, whose job this app's Nightly channel now does.

use bolo_store::Channel;
use serde::Serialize;

/// This build's version; a nightly's carries `-nightly.<date>.<run>`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

const REPO: &str = "https://github.com/Nirav1810/bolo";

/// A build's channel, from its version.
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

    /// The channel the user follows: the one picked in About, else this build's own.
    pub fn followed(picked: Option<Channel>) -> Self {
        match picked {
            Some(Channel::Beta) => Variant::Release,
            Some(Channel::Nightly) => Variant::Nightly,
            None => Self::current(),
        }
    }

    pub fn channel(self) -> Channel {
        match self {
            Variant::Release => Channel::Beta,
            Variant::Nightly => Channel::Nightly,
        }
    }

    /// The name the app goes by while this build runs: window titles, the tray.
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

    /// The log file in `%LOCALAPPDATA%\Bolo\data\logs`, one per channel.
    pub fn log_file(self) -> &'static str {
        match self {
            Variant::Release => "desktop.log",
            Variant::Nightly => "desktop-nightly.log",
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

/// The quoted uninstaller path an `UninstallString` names, without its quotes.
#[cfg_attr(not(windows), allow(dead_code))]
fn uninstaller(uninstall_string: &str) -> Option<&str> {
    let s = uninstall_string.trim();
    let s = match s.strip_prefix('"') {
        Some(rest) => rest.split('"').next().unwrap_or(""),
        None => s,
    };
    (!s.is_empty()).then_some(s)
}

/// Remove the separate Bolo Nightly app, if it is still installed: ask it to let go of
/// the push-to-talk key now, then run its uninstaller quietly. The uninstaller closes it
/// (only that exe: Windows' Restart Manager goes by path) and removes its shortcuts and
/// its start-with-Windows entry; the shared settings, models and history stay. Returns
/// whether it found one.
pub fn retire_old_nightly() -> bool {
    #[cfg(windows)]
    {
        let Some(command) = win::uninstall_string("Bolo Nightly") else { return false };
        let Some(exe) = uninstaller(&command) else { return false };
        let exe = std::path::Path::new(exe);
        // Never the install that runs.
        let here = std::env::current_exe().ok().and_then(|p| p.parent().map(std::path::Path::to_path_buf));
        if !exe.exists() || here.as_deref() == exe.parent() {
            return false;
        }
        win::signal("Local\\dev.bolo.desktop.nightly.step-aside");
        match std::process::Command::new(exe).arg("/S").spawn() {
            Ok(_) => tracing::info!(uninstaller = %exe.display(), "removing the separate Bolo Nightly app"),
            Err(e) => tracing::warn!(%e, "could not remove the separate Bolo Nightly app"),
        }
        true
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[cfg(windows)]
mod win {
    use windows_sys::Win32::Foundation::{CloseHandle, ERROR_SUCCESS};
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_SZ, RegGetValueW};
    use windows_sys::Win32::System::Threading::{EVENT_MODIFY_STATE, OpenEventW, SetEvent};

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    /// Set a named event if anyone made it; whether someone had.
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

    /// `UninstallString` of a per-user app in Apps & features.
    pub fn uninstall_string(product: &str) -> Option<String> {
        let key = wide(&format!("Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{product}"));
        let value = wide("UninstallString");
        let mut buf = vec![0u16; 1024];
        let mut bytes = (buf.len() * 2) as u32;
        // SAFETY: the strings are NUL-terminated and outlive the call; `buf` holds `bytes`
        // bytes, and RRF_RT_REG_SZ makes the result NUL-terminated within them.
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                value.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buf.as_mut_ptr().cast(),
                &mut bytes,
            )
        };
        if status != ERROR_SUCCESS {
            return None;
        }
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Some(String::from_utf16_lossy(&buf[..len]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_version_says_which_channel_built_it() {
        assert_eq!(Variant::of("0.4.0"), Variant::Release);
        assert_eq!(Variant::of("0.5.0-nightly.20261111.7"), Variant::Nightly);
        assert_eq!(Variant::Nightly.product_name(), "Bolo Nightly");
        assert!(Variant::Nightly.feed().ends_with("/releases/download/nightly/latest.json"));
        assert!(Variant::Release.feed().ends_with("/releases/latest/download/latest.json"));
        assert_ne!(Variant::Release.log_file(), Variant::Nightly.log_file());
    }

    #[test]
    fn the_picked_channel_wins_over_the_build() {
        assert_eq!(Variant::followed(Some(Channel::Nightly)), Variant::Nightly);
        assert_eq!(Variant::followed(Some(Channel::Beta)), Variant::Release);
        assert_eq!(Variant::followed(None), Variant::current(), "unpicked: the build's own");
        for v in [Variant::Release, Variant::Nightly] {
            assert_eq!(Variant::followed(Some(v.channel())), v);
        }
    }

    #[test]
    fn badges_and_titles_name_the_channel() {
        assert_eq!(badge("0.4.0"), Some("Beta"));
        assert_eq!(badge("0.5.0-nightly.20261111.7"), Some("Nightly"));
        assert_eq!(badge("1.0.0"), None);
        assert_eq!(title("0.4.0"), "Bolo 0.4.0 · Beta");
        assert_eq!(title("1.0.0"), "Bolo 1.0.0");
        assert_eq!(title("0.5.0-nightly.20261111.7"), "Bolo Nightly 0.5.0-nightly.20261111.7");
    }

    #[test]
    fn the_uninstaller_is_read_from_its_uninstall_string() {
        let quoted = r#""C:\Users\me\AppData\Local\Bolo Nightly\uninstall.exe""#;
        assert_eq!(uninstaller(quoted), Some(r"C:\Users\me\AppData\Local\Bolo Nightly\uninstall.exe"));
        assert_eq!(uninstaller(r"C:\Bolo\uninstall.exe"), Some(r"C:\Bolo\uninstall.exe"));
        assert_eq!(uninstaller(r#"  "C:\B N\uninstall.exe" /S"#), Some(r"C:\B N\uninstall.exe"));
        assert_eq!(uninstaller("  "), None);
        assert_eq!(uninstaller(r#""""#), None);
    }
}
