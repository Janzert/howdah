//! Keeping engines' memory in check: what each running engine holds, the
//! budget all of them share, and when one has grown enough to restart.
//!
//! Each engine's resident memory is read (from the OS, through
//! `howdah_aei::process`) every [`POLL`] while it plays, and after each of
//! its games. Nothing reads it in a loop of its own: an engine that isn't
//! playing keeps its last reading, and slots don't keep idle engines long
//! (`runner::IDLE_KEEP`). The design is in `docs/TOURNAMENTS.md`.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use crate::spec::MemorySpec;

/// How often a running game's engines are read.
pub(crate) const POLL: Duration = Duration::from_secs(2);

/// Slots start no game while the engines hold more than this share of the
/// budget, in percent.
const ROOM_PERCENT: u64 = 90;

/// The budget when none is given: half the machine's memory, or 4 GB if
/// that can't be read.
pub fn default_budget() -> u64 {
    howdah_aei::process::physical_memory().map_or(4 << 30, |p| p / 2)
}

/// Megabytes, rounded, for messages and the results log.
pub(crate) fn mb(bytes: u64) -> u64 {
    (bytes + (1 << 19)) >> 20
}

/// Bytes as people read them: "812 MB", "5.4 GB".
pub(crate) fn size(bytes: u64) -> String {
    if bytes >= 1 << 30 {
        format!("{:.1} GB", bytes as f64 / (1u64 << 30) as f64)
    } else {
        format!("{} MB", mb(bytes))
    }
}

/// The engines a runner or daemon has running, with their last readings,
/// and the budget they share.
#[derive(Debug)]
pub(crate) struct Engines {
    budget: u64,
    held: Mutex<Held>,
}

#[derive(Debug, Default)]
struct Held {
    next: u64,
    /// By key: the engine's pid and its last reading.
    engines: HashMap<u64, (u32, u64)>,
}

impl Engines {
    pub(crate) fn new(budget: u64) -> Engines {
        Engines { budget, held: Mutex::default() }
    }

    pub(crate) fn budget(&self) -> u64 {
        self.budget
    }

    /// Starts tracking an engine; the key is for the calls below.
    pub(crate) fn add(&self, pid: u32) -> u64 {
        let mut h = self.held.lock().unwrap();
        h.next += 1;
        let key = h.next;
        let rss = howdah_aei::process::resident_memory(pid).unwrap_or(0);
        h.engines.insert(key, (pid, rss));
        key
    }

    pub(crate) fn count(&self) -> u32 {
        self.held.lock().unwrap().engines.len() as u32
    }

    pub(crate) fn remove(&self, key: u64) {
        self.held.lock().unwrap().engines.remove(&key);
    }

    /// Reads an engine's resident memory now and keeps it as its reading.
    pub(crate) fn read(&self, key: u64) -> Option<u64> {
        let pid = self.held.lock().unwrap().engines.get(&key)?.0;
        let rss = howdah_aei::process::resident_memory(pid)?;
        if let Some(e) = self.held.lock().unwrap().engines.get_mut(&key) {
            e.1 = rss;
        }
        Some(rss)
    }

    /// What every engine held at its last reading.
    pub(crate) fn total(&self) -> u64 {
        self.held.lock().unwrap().engines.values().map(|e| e.1).sum()
    }

    /// Whether there's room in the budget to start a game.
    pub(crate) fn room(&self) -> bool {
        self.total() < self.budget / 100 * ROOM_PERCENT
    }

    /// If the engines together are over the budget: the total and the
    /// key of the largest.
    pub(crate) fn over_budget(&self) -> Option<(u64, u64)> {
        let h = self.held.lock().unwrap();
        let total: u64 = h.engines.values().map(|e| e.1).sum();
        if total <= self.budget {
            return None;
        }
        let largest = h.engines.iter().max_by_key(|(_, e)| e.1)?;
        Some((total, *largest.0))
    }
}

/// A test's memory limits in bytes, from its spec and the machine.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Limits {
    pub growth: f64,
    pub restart: Option<u64>,
    /// Past this in a game, the engine forfeits it.
    pub limit: u64,
}

impl Limits {
    pub(crate) fn new(spec: &MemorySpec) -> Limits {
        Limits {
            growth: spec.growth,
            restart: spec.restart_mb.map(|mb| mb << 20),
            limit: spec.limit_bytes(howdah_aei::process::physical_memory()),
        }
    }

    /// The cap for the kernel to enforce: a quarter above the limit, and
    /// at least 256 MB above, so the watchdog normally ends the game first
    /// and says why.
    pub(crate) fn kernel_cap(&self) -> u64 {
        self.limit.saturating_add((self.limit / 4).max(256 << 20))
    }

    /// Why an engine using `rss` after a game should be restarted, given
    /// `first`, what it used after its first game.
    pub(crate) fn restart_reason(&self, first: u64, rss: u64) -> Option<String> {
        if let Some(r) = self.restart
            && rss > r
        {
            return Some(format!("memory: {} resident, over its restart limit of {}", size(rss), size(r)));
        }
        if self.growth > 0.0
            && rss >= MemorySpec::GROWTH_FLOOR_MB << 20
            && rss as f64 > first as f64 * self.growth
        {
            return Some(format!(
                "memory: {} resident, up from {} after its first game (more than {}x)",
                size(rss),
                size(first),
                self.growth
            ));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MB: u64 = 1 << 20;

    #[test]
    fn restarts_on_growth_or_size() {
        let l = Limits { growth: 3.0, restart: Some(2000 * MB), limit: 4000 * MB };
        assert_eq!(l.restart_reason(100 * MB, 290 * MB), None);
        let grown = l.restart_reason(100 * MB, 310 * MB).unwrap();
        assert!(grown.contains("310 MB resident, up from 100 MB"), "{grown}");
        // Below the floor, growth alone doesn't count.
        assert_eq!(l.restart_reason(10 * MB, 200 * MB), None);
        let big = l.restart_reason(1900 * MB, 2100 * MB).unwrap();
        assert!(big.contains("2.1 GB resident, over its restart limit of 2.0 GB"), "{big}");
        let off = Limits { growth: 0.0, restart: None, ..l };
        assert_eq!(off.restart_reason(100 * MB, 3000 * MB), None);
        assert_eq!(l.kernel_cap(), 5000 * MB);
        assert_eq!(Limits { limit: 100 * MB, ..l }.kernel_cap(), 356 * MB);
    }

    #[test]
    fn tracks_the_budget() {
        let engines = Engines::new(1000 * MB);
        let a = engines.add(u32::MAX);
        let b = engines.add(u32::MAX);
        let set = |key, rss| engines.held.lock().unwrap().engines.get_mut(&key).unwrap().1 = rss;
        set(a, 300 * MB);
        set(b, 500 * MB);
        assert!(engines.room());
        assert_eq!(engines.over_budget(), None);
        set(b, 620 * MB);
        assert!(!engines.room());
        assert_eq!(engines.over_budget(), None);
        set(b, 720 * MB);
        assert_eq!(engines.over_budget(), Some((1020 * MB, b)));
        engines.remove(b);
        assert_eq!(engines.total(), 300 * MB);
    }
}
