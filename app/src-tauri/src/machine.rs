//! What this computer is like, so the defaults that depend on it (how many tracks are analysed at
//! once, whether analysis runs below normal priority, whether it waits on battery) follow the
//! machine instead of being tuned to one PC. Every function answers a safe middle value where the
//! system gives no answer, and the Windows calls sit behind `cfg` so another system builds with
//! the fallbacks until its own versions are written.

use std::sync::atomic::{AtomicBool, Ordering};

/// Logical processors the system reports (2 when it does not say).
pub fn logical_cores() -> usize {
    std::thread::available_parallelism().map_or(2, usize::from)
}

/// Memory that can be used without making the system swap, in MB. None when the system does not say.
#[cfg(windows)]
pub fn available_ram_mb() -> Option<u64> {
    use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut s = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    // SAFETY: `s` is a valid MEMORYSTATUSEX with dwLength set, as the call requires.
    unsafe { GlobalMemoryStatusEx(&mut s) }.ok()?;
    Some(s.ullAvailPhys / (1024 * 1024))
}

#[cfg(not(windows))]
pub fn available_ram_mb() -> Option<u64> {
    None
}

/// All the physical memory of this computer, in MB. None when the system does not say.
#[cfg(windows)]
pub fn total_ram_mb() -> Option<u64> {
    use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut s = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    // SAFETY: `s` is a valid MEMORYSTATUSEX with dwLength set, as the call requires.
    unsafe { GlobalMemoryStatusEx(&mut s) }.ok()?;
    Some(s.ullTotalPhys / (1024 * 1024))
}

#[cfg(not(windows))]
pub fn total_ram_mb() -> Option<u64> {
    None
}

/// The live mix player's decoded-audio budget when the user has not chosen one: an eighth of the
/// computer's memory, from 256 MB (a few tracks) to 1024 MB (the budget every computer had before
/// it followed the machine, about seven 6-minute tracks). A computer that does not say gets 1024.
pub fn auto_decode_budget_mb(total_ram_mb: Option<u64>) -> u64 {
    total_ram_mb.map_or(1024, |t| (t / 8).clamp(256, 1024))
}

/// Running on the battery of a laptop or tablet. A desktop, or a system that does not say, is
/// never "on battery".
#[cfg(windows)]
pub fn on_battery() -> bool {
    use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
    let mut s = SYSTEM_POWER_STATUS::default();
    // SAFETY: `s` is a valid out structure. ACLineStatus 0 = offline, 1 = online, 255 = unknown;
    // BatteryFlag 128 = no battery, 255 = unknown.
    if unsafe { GetSystemPowerStatus(&mut s) }.is_err() {
        return false;
    }
    s.ACLineStatus == 0 && s.BatteryFlag != 128 && s.BatteryFlag != 255
}

#[cfg(not(windows))]
pub fn on_battery() -> bool {
    false
}

/// How many tracks to analyse at once on this computer, right now: see
/// [`pm_analysis::workers::auto_workers`].
pub fn analysis_workers() -> usize {
    pm_analysis::workers::analysis_workers()
}

/// The tracks to analyse at once for a saved setting: 0 means "decide by the computer".
pub fn workers_for(setting: usize) -> usize {
    if setting == 0 { analysis_workers() } else { setting }
}

/// Lets the analysis crate ask this computer for its free memory (called once at start-up).
pub fn register() {
    pm_analysis::workers::set_ram_probe(available_ram_mb);
    pm_analysis::workers::set_thread_start_hook(job_thread_start);
}

/// Puts the calling thread below normal priority (or back to normal). Windows only for now; the
/// other systems keep the thread as it is.
#[cfg(windows)]
pub fn set_thread_low_priority(low: bool) {
    use windows::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_BELOW_NORMAL, THREAD_PRIORITY_NORMAL};
    // SAFETY: the pseudo handle of the calling thread is always valid.
    let r = unsafe { SetThreadPriority(GetCurrentThread(), if low { THREAD_PRIORITY_BELOW_NORMAL } else { THREAD_PRIORITY_NORMAL }) };
    if let Err(e) = r {
        eprintln!("machine: could not change the thread priority: {e}");
    }
}

#[cfg(not(windows))]
pub fn set_thread_low_priority(_low: bool) {}

/// Whether the worker threads of the analysis jobs the user starts (energy, sections, duplicate
/// check, grid analysis) run below normal priority: the same switch as for the background worker
/// (Settings > Analysis > Analyse at low priority), kept here so a job thread can read it without
/// the settings file.
static JOBS_LOW_PRIORITY: AtomicBool = AtomicBool::new(false);

pub fn set_jobs_low_priority(low: bool) {
    JOBS_LOW_PRIORITY.store(low, Ordering::Relaxed);
}

/// Called first thing by a thread of a user-started analysis job.
pub fn job_thread_start() {
    if JOBS_LOW_PRIORITY.load(Ordering::Relaxed) {
        set_thread_low_priority(true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_live_mix_budget_follows_memory() {
        assert_eq!(auto_decode_budget_mb(None), 1024);
        assert_eq!(auto_decode_budget_mb(Some(2048)), 256, "never under 256 MB");
        assert_eq!(auto_decode_budget_mb(Some(4096)), 512);
        assert_eq!(auto_decode_budget_mb(Some(8192)), 1024);
        assert_eq!(auto_decode_budget_mb(Some(65536)), 1024, "and never over what every computer had before");
    }

    #[test]
    fn this_computer_gives_sane_answers() {
        assert!(logical_cores() >= 1);
        register();
        assert!((1..=pm_analysis::workers::MAX_AUTO_WORKERS).contains(&analysis_workers()));
        let _ = on_battery();
        assert!((256..=1024).contains(&auto_decode_budget_mb(total_ram_mb())));
        if let Some(mb) = available_ram_mb() {
            assert!(mb > 0);
        }
    }
}
