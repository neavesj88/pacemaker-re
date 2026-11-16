//! How many tracks to analyse at once. Every analysis job holds one whole decoded track per
//! worker (about 10 MB a minute of audio at 44.1 kHz), so the right number depends on the
//! machine: its logical processors, and how much memory is free. The app registers a probe for
//! free memory (it needs the operating system's calls); without one only the processors count.

use std::sync::OnceLock;

/// Memory one worker is planned for: a decoded track of up to half an hour plus working buffers.
const WORKER_MB: u64 = 400;

/// The most workers chosen automatically. More helps little (the decoders and the disk give out
/// first) and each holds a whole track in memory. A number saved in Settings can go higher.
pub const MAX_AUTO_WORKERS: usize = 4;

/// How many tracks to analyse at once on a machine with `cores` logical processors and `ram_mb`
/// of free memory: one processor is left for the app (a two-processor machine uses both, because
/// measured on a pair of hyper-threads two workers still finish a job 1.37 times faster than one,
/// and the analysis runs below normal priority there so the app keeps its share), and no more
/// workers than half the free memory holds. Never fewer than one, never more than
/// [`MAX_AUTO_WORKERS`].
pub fn auto_workers(cores: usize, ram_mb: Option<u64>) -> usize {
    let by_cores = if cores <= 2 { cores } else { cores - 1 };
    let by_ram = ram_mb.map_or(usize::MAX, |mb| (mb / 2 / WORKER_MB) as usize);
    by_cores.min(by_ram).clamp(1, MAX_AUTO_WORKERS)
}

static RAM_PROBE: OnceLock<fn() -> Option<u64>> = OnceLock::new();

/// Registers the way to ask the system for free memory in MB (the first registration wins).
pub fn set_ram_probe(probe: fn() -> Option<u64>) {
    let _ = RAM_PROBE.set(probe);
}

static START_HOOK: OnceLock<fn()> = OnceLock::new();

/// Registers what every analysis worker thread does first (the app lowers its priority there
/// when the user asked for that); the first registration wins.
pub fn set_thread_start_hook(hook: fn()) {
    let _ = START_HOOK.set(hook);
}

/// Called first thing by every analysis worker thread.
pub fn thread_start() {
    if let Some(hook) = START_HOOK.get() {
        hook();
    }
}

/// Free memory in MB right now, when the app registered a way to ask the system.
pub fn free_ram_mb() -> Option<u64> {
    RAM_PROBE.get().and_then(|p| p())
}

/// [`auto_workers`] for this computer, right now.
pub fn analysis_workers() -> usize {
    let cores = std::thread::available_parallelism().map_or(2, usize::from);
    auto_workers(cores, free_ram_mb())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workers_follow_cores_and_memory() {
        // a single processor does one at a time, two processors use both, then one is left over
        assert_eq!(auto_workers(1, None), 1);
        assert_eq!(auto_workers(2, None), 2);
        assert_eq!(auto_workers(3, None), 2);
        assert_eq!(auto_workers(4, None), 3);
        // bigger machines stop at the ceiling
        assert_eq!(auto_workers(5, None), 4);
        assert_eq!(auto_workers(6, None), 4);
        assert_eq!(auto_workers(24, None), 4);
        // memory caps it: 1.5 GB free holds one worker's 400 MB twice over, not four
        assert_eq!(auto_workers(24, Some(1500)), 1);
        assert_eq!(auto_workers(24, Some(2400)), 3);
        assert_eq!(auto_workers(24, Some(3300)), 4);
        assert_eq!(auto_workers(24, Some(100)), 1, "never fewer than one");
        assert_eq!(auto_workers(24, Some(64 * 1024)), 4);
    }

    #[test]
    fn this_computer_gives_a_sane_answer() {
        assert!((1..=MAX_AUTO_WORKERS).contains(&analysis_workers()));
    }
}
