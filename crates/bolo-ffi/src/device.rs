//! What the phone has: RAM (for the model tier), big CPU cores (for inference threads)
//! and free storage (before a download). Read from `/proc` and `/sys` on Android; the
//! readers return `None` where those don't exist (the host tests).

use std::path::Path;

use bolo_models::{Manifest, Platform, Tier};

/// Inference threads never exceed this: more cores add heat, not speed (race to idle).
const MAX_THREADS: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Device {
    /// `MemTotal` in MiB: what `ActivityManager.MemoryInfo.totalMem` reports.
    pub total_ram_mb: Option<u64>,
    /// Cores that aren't the efficiency cluster.
    pub big_cores: Option<usize>,
}

impl Device {
    pub fn detect() -> Self {
        let device = Self { total_ram_mb: total_ram_mb(), big_cores: big_cores_detected() };
        tracing::info!(ram_mb = ?device.total_ram_mb, big_cores = ?device.big_cores, "device");
        device
    }

    /// The RAM to pick models by. Unknown counts as plenty: high tier, and the user's
    /// engine choice stands.
    pub fn ram_mb(&self) -> u64 {
        self.total_ram_mb.unwrap_or(u64::MAX)
    }

    /// The phone's model tier.
    pub fn tier(&self, manifest: &Manifest) -> Tier {
        manifest.tier(Platform::Android, self.ram_mb())
    }

    /// Inference threads for local models: `min(4, big cores)`. The little cores would
    /// slow the slowest thread down, and every thread waits for it.
    pub fn threads(&self) -> i32 {
        let cores = self.big_cores.unwrap_or_else(|| std::thread::available_parallelism().map_or(2, |n| n.get()));
        cores.clamp(1, MAX_THREADS) as i32
    }
}

/// `MemTotal` from `/proc/meminfo`, in MiB.
fn total_ram_mb() -> Option<u64> {
    parse_mem_total(&std::fs::read_to_string("/proc/meminfo").ok()?)
}

pub(crate) fn parse_mem_total(meminfo: &str) -> Option<u64> {
    let line = meminfo.lines().find_map(|l| l.strip_prefix("MemTotal:"))?;
    let kb: u64 = line.trim().strip_suffix("kB")?.trim().parse().ok()?;
    Some(kb / 1024)
}

/// Big cores from each core's `cpuinfo_max_freq`.
fn big_cores_detected() -> Option<usize> {
    let freqs: Vec<u64> = std::fs::read_dir("/sys/devices/system/cpu")
        .ok()?
        .filter_map(Result::ok)
        .filter(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            name.strip_prefix("cpu").is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        })
        .filter_map(|e| std::fs::read_to_string(e.path().join("cpufreq/cpuinfo_max_freq")).ok())
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    big_cores(&freqs)
}

/// The cores faster than the slowest cluster: on a 1+3+4 phone, the 4 big ones; on a
/// 2+6 phone, 2. All cores when they're all the same (one cluster, or the emulator).
pub(crate) fn big_cores(max_freqs: &[u64]) -> Option<usize> {
    let slowest = *max_freqs.iter().min()?;
    let big = max_freqs.iter().filter(|&&f| f > slowest).count();
    Some(if big == 0 { max_freqs.len() } else { big })
}

/// Free bytes for an app on the file system that holds `path`.
#[cfg(unix)]
pub(crate) fn free_bytes(path: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: `c` is a valid NUL-terminated path and `st` a writable statvfs.
    if unsafe { libc::statvfs(c.as_ptr(), &mut st) } != 0 {
        return None;
    }
    #[allow(clippy::unnecessary_cast)] // the field types differ between targets
    let bytes = st.f_bavail as u64 * st.f_frsize as u64;
    Some(bytes)
}

#[cfg(not(unix))]
pub(crate) fn free_bytes(_path: &Path) -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_mem_total_in_mib() {
        let emulator = "MemTotal:        6072060 kB\nMemFree:          123456 kB\n";
        assert_eq!(parse_mem_total(emulator), Some(5929));
        assert_eq!(parse_mem_total("MemFree: 1 kB\n"), None);
        assert_eq!(parse_mem_total("MemTotal: lots\n"), None);
    }

    #[test]
    fn big_cores_leave_out_the_efficiency_cluster() {
        assert_eq!(big_cores(&[]), None);
        // Prime + 3 big + 4 little (Snapdragon 8 Gen 2 style).
        assert_eq!(big_cores(&[2016000, 2016000, 2016000, 2016000, 2803200, 2803200, 2803200, 3187200]), Some(4));
        // 2 big + 6 little (mid-range).
        assert_eq!(big_cores(&[1800000; 6].iter().chain(&[2400000; 2]).copied().collect::<Vec<_>>()), Some(2));
        // One cluster, or the emulator's identical cores.
        assert_eq!(big_cores(&[2000000; 4]), Some(4));
    }

    #[test]
    fn threads_are_at_most_four_big_cores() {
        let d = |big| Device { total_ram_mb: Some(6000), big_cores: Some(big) };
        assert_eq!(d(2).threads(), 2);
        assert_eq!(d(4).threads(), 4);
        assert_eq!(d(7).threads(), 4);
        assert_eq!(d(0).threads(), 1);
        assert!((1..=4).contains(&Device { total_ram_mb: None, big_cores: None }.threads()));
    }

    #[test]
    fn the_tier_follows_the_reported_ram() {
        let m = Manifest::bundled().unwrap();
        let tier = |ram| Device { total_ram_mb: ram, big_cores: None }.tier(&m);
        assert_eq!(tier(Some(5929)), Tier::AndroidHigh, "the 6 GB emulator");
        assert_eq!(tier(Some(3700)), Tier::AndroidLow, "a 4 GB phone");
        assert_eq!(tier(None), Tier::AndroidHigh, "unknown: the user's choice stands");
    }

    #[test]
    fn free_bytes_of_the_temp_dir() {
        let free = free_bytes(&std::env::temp_dir());
        if cfg!(unix) {
            assert!(free.is_some_and(|b| b > 0));
        } else {
            assert_eq!(free, None);
        }
    }
}
