//! Peak resident memory while an engine is loaded and running.
//!
//! A background thread samples this process's RSS every 10 ms; we report the peak
//! minus the baseline at start. Sampling can miss a sub-10 ms spike, which is fine for
//! model-sized allocations.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

fn current_rss(sys: &mut System, pid: Pid) -> u64 {
    sys.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), true, ProcessRefreshKind::nothing().with_memory());
    sys.process(pid).map_or(0, |p| p.memory())
}

pub fn total_ram_mb() -> u64 {
    let Ok(mut sys) = System::new() else { return 0 };
    sys.refresh_memory();
    sys.total_memory() / (1024 * 1024)
}

pub struct PeakSampler {
    stop: Arc<AtomicBool>,
    peak: Arc<AtomicU64>,
    baseline: u64,
    handle: Option<JoinHandle<()>>,
}

impl PeakSampler {
    pub fn start() -> Self {
        let pid = sysinfo::get_current_pid().unwrap_or(Pid::from_u32(std::process::id()));
        let mut sys = System::new().ok();
        let baseline = sys.as_mut().map_or(0, |s| current_rss(s, pid));
        let stop = Arc::new(AtomicBool::new(false));
        let peak = Arc::new(AtomicU64::new(baseline));
        let (s, p) = (stop.clone(), peak.clone());
        let handle = std::thread::spawn(move || {
            while !s.load(Ordering::Relaxed) {
                if let Some(sys) = sys.as_mut() {
                    p.fetch_max(current_rss(sys, pid), Ordering::Relaxed);
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        });
        Self { stop, peak, baseline, handle: Some(handle) }
    }

    /// Stop sampling; returns (peak − baseline) in MB.
    pub fn stop_delta_mb(mut self) -> f64 {
        self.finish();
        self.peak.load(Ordering::Relaxed).saturating_sub(self.baseline) as f64 / (1024.0 * 1024.0)
    }

    fn finish(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for PeakSampler {
    fn drop(&mut self) {
        self.finish();
    }
}
