//! The daemon's queue: tests sharing a number of game slots.
//!
//! Tests live in `<data dir>/tests/<id>/`, each a [`TestDir`] plus
//! `state.json` (its [`TestStatus`], priority, pair limit and end), so the
//! queue survives a restart: tests that were active carry on. A free slot
//! takes a game from the active test with the highest priority, staying on
//! the test it holds engines for when that's among the best, and otherwise
//! the one with the fewest games running. A test's `concurrency` caps its
//! games at once. All the slots' engines share a memory budget (see
//! `memory`).
//!
//! A test that isn't active and has no games running can be taken out of
//! the queue: archived (zipped into `<data dir>/archive/<id>.zip`, which
//! unzipped back into `tests/` brings it back on the next start) or
//! deleted. Archived tests can be listed, restored (unzipped back into
//! the queue as they were) or deleted.
//!
//! This is the daemon without its HTTP API (`server`), so it can be driven
//! and tested directly.

use std::cmp::Reverse;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use howdah_arimaa::Color;
use serde::{Deserialize, Serialize};
use tokio::sync::{Notify, broadcast};
use tokio::task::JoinHandle;

use crate::error::TourneyError;
use crate::memory::{self, Engines};
use crate::pool::PoolInfo;
use crate::results::{GameEntry, parse_results};
use crate::runner::{EngineLine, EventFn, LiveGame, Picker, RunEnd, RunEvent, TestRun, play_slot};
use crate::spec::{EngineSpec, SprtSpec, TestSpec};
use crate::spsa::{SpsaInfo, SpsaStep};
use crate::stats::{Projection, Summary};
use crate::testdir::{TestDir, slug};

/// Overrides the data directory.
pub const DATA_DIR_ENV: &str = "HOWDAH_TOURNEY_DIR";

const STATE_FILE: &str = "state.json";

/// Where archived tests go, in the data directory.
pub const ARCHIVE_DIR: &str = "archive";

/// How long taking a test out waits for slots to let its engines go.
const RETIRE_WAIT: Duration = Duration::from_secs(30);

/// In the data directory while a daemon serves it: how to reach it.
pub const INFO_FILE: &str = "daemon.json";

/// What `daemon.json` holds. Only the user can read it, and clients send
/// the token with every request, so other users on the machine can't
/// drive the daemon.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DaemonInfo {
    /// On 127.0.0.1.
    pub port: u16,
    pub token: String,
    pub pid: u32,
    pub version: String,
}

impl DaemonInfo {
    /// The daemon's info, if one has written it (it may have died since).
    pub fn read(data_dir: &Path) -> Option<DaemonInfo> {
        serde_json::from_str(&std::fs::read_to_string(data_dir.join(INFO_FILE)).ok()?).ok()
    }
}

/// `$HOWDAH_TOURNEY_DIR`, else `tourney/` in Howdah's local data directory
/// (where the app keeps its installed engines).
pub fn default_data_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os(DATA_DIR_ENV) {
        return Some(dir.into());
    }
    Some(dirs::data_local_dir()?.join("io.github.janzert.howdah").join("tourney"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TestStatus {
    /// Wants games.
    Active,
    /// Starts no games until resumed.
    Paused,
    /// Ended (see its `end`); running games still finish.
    Finished,
}

/// What `state.json` holds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SavedState {
    status: TestStatus,
    priority: i32,
    max_pairs: Option<u32>,
    /// Games at once, when changed from the spec's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    concurrency: Option<u32>,
    /// The SPRT's `undecided_below`, when changed from the spec's (0:
    /// off).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    undecided_below: Option<f64>,
    /// Seconds since the Unix epoch.
    submitted: u64,
    end: Option<RunEnd>,
}

impl SavedState {
    /// The SPRT's chance of deciding within the pair limit below which
    /// the test ends undecided, if it does.
    fn undecided_below(&self, spec: &TestSpec) -> Option<f64> {
        match self.undecided_below {
            Some(c) => (c > 0.0).then_some(c),
            None => spec.sprt.and_then(|s| s.undecided_below),
        }
    }
}

/// A test as clients see it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestInfo {
    pub id: String,
    pub name: String,
    pub engines: [String; 2],
    pub time_control: Option<String>,
    pub status: TestStatus,
    pub priority: i32,
    pub max_pairs: Option<u32>,
    pub sprt: Option<SprtSpec>,
    pub concurrency: u32,
    /// Games being played (pair, game).
    pub running: Vec<(u32, u8)>,
    /// In a pool, the engines (1, 2) of each of `running`'s pairs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub running_engines: Vec<[String; 2]>,
    pub end: Option<RunEnd>,
    pub submitted: u64,
    pub summary: Summary,
    /// The SPRT's chance of deciding within the pair limit below which
    /// the test ends undecided (the spec's `undecided_below`, unless
    /// changed); None: it plays on to the limit.
    #[serde(default)]
    pub undecided_below: Option<f64>,
    /// The chance the SPRT still reaches a bound within the pair limit
    /// ([`crate::stats::decide_chance`]); None without both, or once it
    /// has decided.
    #[serde(default)]
    pub decide_chance: Option<f64>,
    /// How an unfinished SPRT test is likely to end, and when
    /// ([`crate::stats::project`]); None without an SPRT, or once it has
    /// decided or finished.
    #[serde(default)]
    pub projection: Option<Projection>,
    /// An SPSA run's iterations and parameters (its `engines` are the two
    /// sides: θ + cΔ and θ − cΔ).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spsa: Option<SpsaInfo>,
    /// A pool's engines, ratings and matchups (its `engines` are its first
    /// two, and its `summary` means nothing).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pool: Option<PoolInfo>,
    /// The test's directory.
    pub path: PathBuf,
}

impl TestInfo {
    fn with_decide_chance(mut self) -> Self {
        self.decide_chance = self
            .sprt
            .zip(self.max_pairs)
            .and_then(|(sp, max)| crate::stats::decide_chance(sp, &self.summary, max));
        self
    }
}

/// A test in the archive.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchivedTest {
    /// The zip's file name in `archive/`, which names it to the daemon.
    pub file: String,
    /// The zip's size in bytes.
    pub size: u64,
    /// When it was archived, in seconds since the Unix epoch.
    pub archived: u64,
    /// The test as it was (its path is the zip's, and nothing is
    /// running); None if the zip couldn't be read.
    pub test: Option<TestInfo>,
    /// Why it couldn't be read.
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunningGame {
    pub test: String,
    pub pair: u32,
    pub game: u8,
    pub gold: String,
    pub silver: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DaemonStatus {
    pub slots: u32,
    pub running: Vec<RunningGame>,
    pub tests: u32,
    pub data_dir: PathBuf,
    pub pid: u32,
    pub closing: bool,
    /// Bytes all the engines may hold together.
    #[serde(default)]
    pub memory_budget: u64,
    /// Bytes the engines held at their last readings.
    #[serde(default)]
    pub engine_memory: u64,
    /// Engine processes running (each slot's, idle ones included).
    #[serde(default)]
    pub engines: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum DaemonEvent {
    /// A test was added, changed state, or finished a game.
    TestChanged {
        test: Box<TestInfo>,
    },
    GameStarted {
        test: String,
        pair: u32,
        game: u8,
    },
    /// A move in a running game, with both reserves after it (gold,
    /// silver) if it has clocks.
    MovePlayed {
        test: String,
        pair: u32,
        game: u8,
        ply: usize,
        notation: String,
        used_ms: u64,
        reserves_ms: Option<[u64; 2]>,
    },
    GameFinished {
        test: String,
        entry: GameEntry,
    },
    /// `engine` is its side in the pair, `name` its name.
    EngineRestarted {
        test: String,
        engine: usize,
        #[serde(default)]
        name: String,
        reason: String,
    },
    EngineFailedToStart {
        test: String,
        engine: usize,
        #[serde(default)]
        name: String,
        error: String,
    },
    /// A pool's engine was retired for failing game after game.
    EngineRetired {
        test: String,
        name: String,
        reason: String,
    },
    /// A test was taken out of the queue: archived (to `archive`) or
    /// deleted.
    TestRemoved {
        test: String,
        archive: Option<PathBuf>,
    },
    SlotsChanged {
        slots: u32,
    },
    /// No more games start; with `now`, running games are abandoned.
    ShuttingDown {
        now: bool,
    },
    /// A shutdown that let the running games finish was called off.
    ShutdownCancelled,
}

/// An engine's search output in a running game, from the game's output
/// stream (kept apart from [`DaemonEvent`]s, which every client gets).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameOutput {
    pub test: String,
    pub pair: u32,
    pub game: u8,
    pub side: Color,
    pub line: EngineLine,
}

impl DaemonEvent {
    /// The event's `type`, as the server names its server-sent events.
    pub fn name(&self) -> &'static str {
        match self {
            DaemonEvent::TestChanged { .. } => "testChanged",
            DaemonEvent::GameStarted { .. } => "gameStarted",
            DaemonEvent::MovePlayed { .. } => "movePlayed",
            DaemonEvent::GameFinished { .. } => "gameFinished",
            DaemonEvent::EngineRestarted { .. } => "engineRestarted",
            DaemonEvent::EngineFailedToStart { .. } => "engineFailedToStart",
            DaemonEvent::EngineRetired { .. } => "engineRetired",
            DaemonEvent::TestRemoved { .. } => "testRemoved",
            DaemonEvent::SlotsChanged { .. } => "slotsChanged",
            DaemonEvent::ShuttingDown { .. } => "shuttingDown",
            DaemonEvent::ShutdownCancelled => "shutdownCancelled",
        }
    }
}

struct Entry {
    id: String,
    run: Arc<TestRun>,
    saved: SavedState,
    projection: Arc<ProjectionCache>,
}

/// A test's last projection, kept since it takes a while to work out:
/// it's worked out again only when what it depends on changes, and
/// outside the queue's lock.
#[derive(Default)]
struct ProjectionCache(Mutex<Option<(ProjectionKey, Option<Projection>)>>);

/// What a projection depends on: the pair counts, games, pair limit,
/// cutoff, and whether the test can still end by the SPRT.
type ProjectionKey = ([u32; 3], u32, Option<u32>, Option<u64>, bool);

/// A test's info without its projection, with where to find it.
struct InfoParts(TestInfo, Arc<ProjectionCache>);

impl InfoParts {
    /// The info with its projection, worked out if it isn't kept. Call it
    /// without the queue's lock held.
    fn finish(self) -> TestInfo {
        let InfoParts(mut info, cache) = self;
        let open = info.status != TestStatus::Finished && info.summary.decision.is_none();
        let key = (
            info.summary.pair_counts,
            info.summary.games,
            info.max_pairs,
            info.undecided_below.map(f64::to_bits),
            open,
        );
        let mut kept = cache.0.lock().unwrap();
        info.projection = match &*kept {
            Some((k, p)) if *k == key => *p,
            _ => {
                let p = info.sprt.filter(|_| open).and_then(|sp| {
                    crate::stats::project(sp, &info.summary, info.max_pairs, info.undecided_below)
                });
                *kept = Some((key, p));
                p
            }
        };
        info
    }
}

impl Entry {
    fn info(&self) -> TestInfo {
        self.info_parts().finish()
    }

    /// [`Entry::info`] without working out the projection, for use under
    /// the queue's lock: call [`InfoParts::finish`] once it's released.
    fn info_parts(&self) -> InfoParts {
        InfoParts(self.info_base(), self.projection.clone())
    }

    fn info_base(&self) -> TestInfo {
        let spec = &self.run.dir.spec;
        TestInfo {
            id: self.id.clone(),
            name: spec.name.clone(),
            engines: spec.side_names(),
            time_control: spec.time_control.clone(),
            status: self.saved.status,
            priority: self.saved.priority,
            max_pairs: self.saved.max_pairs,
            sprt: spec.sprt,
            concurrency: self.run.concurrency(),
            running: self.run.running(),
            running_engines: if spec.is_pool() {
                self.run.running().iter().map(|&(pair, _)| self.run.pair_names(pair)).collect()
            } else {
                Vec::new()
            },
            end: self.saved.end.clone(),
            submitted: self.saved.submitted,
            summary: self.run.summary(),
            undecided_below: self.saved.undecided_below(spec),
            decide_chance: None,
            projection: None,
            spsa: self.run.spsa_info(),
            pool: self.run.pool_info(),
            path: self.run.dir.path.clone(),
        }
        .with_decide_chance()
    }

    fn save(&self) -> Result<(), TourneyError> {
        let path = self.run.dir.path.join(STATE_FILE);
        let tmp = path.with_extension("json.tmp");
        let json = serde_json::to_string_pretty(&self.saved).expect("state always serializes");
        std::fs::write(&tmp, json).map_err(TourneyError::io(&tmp))?;
        std::fs::rename(&tmp, &path).map_err(TourneyError::io(&path))
    }

    /// After a change to the pair limit or the cutoff: a test that ended
    /// for want of room (all pairs played, or unlikely to decide) carries
    /// on if it now has some, and an active one that now can't decide in
    /// time ends.
    fn reconsider(&mut self) {
        let room = match self.saved.end {
            Some(RunEnd::MaxPairs) => true,
            Some(RunEnd::Undecided) => !self.run.unlikely_to_decide() && self.run.clear_halt(),
            _ => false,
        };
        if self.saved.status == TestStatus::Finished && room {
            self.saved.status = TestStatus::Active;
            self.saved.end = None;
            self.run.hold.clear();
        }
        if self.saved.status == TestStatus::Active {
            self.run.end_if_unlikely();
        }
        self.settle();
    }

    /// Marks an active test finished once it has ended.
    fn settle(&mut self) {
        if self.saved.status == TestStatus::Active
            && let Some(end) = self.run.outcome()
        {
            self.saved.status = TestStatus::Finished;
            self.saved.end = Some(end);
            self.run.hold.stop();
        }
    }
}

struct Slots {
    target: u32,
    handles: Vec<JoinHandle<()>>,
    /// Whether slot `i` has a task.
    alive: Vec<bool>,
    /// The daemon has shut down: [`Daemon::closed`] has returned, or is
    /// about to.
    closed: bool,
    /// The shutdown abandoned the running games, so it can't be called off.
    abandoned: bool,
}

struct Inner {
    data_dir: PathBuf,
    tests: Mutex<Vec<Entry>>,
    slots: Mutex<Slots>,
    /// Wakes idle slots when there may be a game for them.
    wake: Notify,
    events: broadcast::Sender<DaemonEvent>,
    /// Engine output from every running game.
    output: broadcast::Sender<GameOutput>,
    closing: AtomicBool,
    engines: Engines,
}

/// The queue and its slots. Cloning shares it.
#[derive(Clone)]
pub struct Daemon(Arc<Inner>);

impl Daemon {
    /// Loads the tests in `data_dir` and starts `slots` slots. Active tests
    /// carry on where they were. The engines' memory budget is
    /// [`crate::default_memory_budget`].
    pub fn start(data_dir: &Path, slots: u32) -> Result<Daemon, TourneyError> {
        Daemon::start_with_budget(data_dir, slots, memory::default_budget())
    }

    /// [`Daemon::start`] with a budget of `memory_budget` bytes for all the
    /// engines together.
    pub fn start_with_budget(
        data_dir: &Path,
        slots: u32,
        memory_budget: u64,
    ) -> Result<Daemon, TourneyError> {
        let tests_dir = data_dir.join("tests");
        std::fs::create_dir_all(&tests_dir).map_err(TourneyError::io(&tests_dir))?;
        let (events, _) = broadcast::channel(1024);
        let (output, _) = broadcast::channel(4096);
        let inner = Arc::new(Inner {
            data_dir: data_dir.to_path_buf(),
            tests: Mutex::new(Vec::new()),
            slots: Mutex::new(Slots {
                target: 0,
                handles: Vec::new(),
                alive: Vec::new(),
                closed: false,
                abandoned: false,
            }),
            wake: Notify::new(),
            events,
            output,
            closing: AtomicBool::new(false),
            engines: Engines::new(memory_budget),
        });
        let mut dirs: Vec<PathBuf> = std::fs::read_dir(&tests_dir)
            .map_err(TourneyError::io(&tests_dir))?
            .filter_map(|d| d.ok().map(|d| d.path()))
            .filter(|p| p.join(crate::testdir::SPEC_FILE).is_file())
            .collect();
        dirs.sort();
        let mut tests = Vec::new();
        for path in dirs {
            match load_entry(&inner, &path) {
                Ok(e) => tests.push(e),
                Err(e) => eprintln!("skipping {}: {e}", path.display()),
            }
        }
        *inner.tests.lock().unwrap() = tests;
        let daemon = Daemon(inner);
        daemon.set_slots(slots);
        Ok(daemon)
    }

    pub fn data_dir(&self) -> &Path {
        &self.0.data_dir
    }

    pub fn subscribe(&self) -> broadcast::Receiver<DaemonEvent> {
        self.0.events.subscribe()
    }

    /// The engines' search output in every running game.
    pub fn subscribe_output(&self) -> broadcast::Receiver<GameOutput> {
        self.0.output.subscribe()
    }

    /// Adds a test. Its engines' programs must be absolute paths (the
    /// daemon doesn't share the submitter's working directory).
    pub fn submit(&self, spec: TestSpec, priority: i32) -> Result<TestInfo, TourneyError> {
        self.refuse_if_closing()?;
        if let Some(e) = spec.engines.iter().find(|e| !e.program.is_absolute()) {
            return Err(TourneyError::Refused(format!(
                "{}'s program must be an absolute path, not {}",
                e.name,
                e.program.display()
            )));
        }
        let tests_dir = self.0.data_dir.join("tests");
        // Claim the next number by making its directory, so two submissions
        // can't take the same one; copying the engines happens unlocked.
        let id = {
            let tests = self.0.tests.lock().unwrap();
            // Archived tests' numbers aren't used again.
            let archived: Vec<String> = std::fs::read_dir(self.0.data_dir.join(ARCHIVE_DIR))
                .into_iter()
                .flatten()
                .filter_map(|d| Some(d.ok()?.file_name().to_str()?.to_string()))
                .collect();
            let mut next = tests
                .iter()
                .map(|e| e.id.as_str())
                .chain(archived.iter().map(String::as_str))
                .filter_map(|id| id.split('-').next()?.parse::<u32>().ok())
                .max()
                .unwrap_or(0)
                + 1;
            loop {
                let id = format!("{next:04}-{}", slug(&spec.name));
                match std::fs::create_dir(tests_dir.join(&id)) {
                    Ok(()) => break id,
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => next += 1,
                    Err(e) => return Err(TourneyError::io(tests_dir.join(&id))(e)),
                }
            }
        };
        let max_pairs = spec.pair_limit();
        let path = tests_dir.join(&id);
        TestDir::create(spec, &self.0.data_dir, &path)?;
        let saved = SavedState {
            status: TestStatus::Active,
            priority,
            max_pairs,
            concurrency: None,
            undecided_below: None,
            submitted: now(),
            end: None,
        };
        let entry = load_entry_with(&self.0, &path, saved)?;
        entry.save()?;
        let info = entry.info();
        self.0.tests.lock().unwrap().push(entry);
        self.changed(&info);
        Ok(info)
    }

    pub fn list(&self) -> Vec<TestInfo> {
        let parts: Vec<_> = self.0.tests.lock().unwrap().iter().map(Entry::info_parts).collect();
        parts.into_iter().map(InfoParts::finish).collect()
    }

    pub fn info(&self, id: &str) -> Result<TestInfo, TourneyError> {
        Ok(self.with(id, |e| Ok(e.info_parts()))?.finish())
    }

    pub fn games(&self, id: &str) -> Result<Vec<GameEntry>, TourneyError> {
        self.with(id, |e| Ok(e.run.entries()))
    }

    /// An SPSA run's steps after the first `skip`, in the order they were
    /// taken (none for a test without SPSA).
    pub fn spsa_history(&self, id: &str, skip: usize) -> Result<Vec<SpsaStep>, TourneyError> {
        let dir = self.with(id, |e| Ok(e.run.dir.spec.spsa.is_some().then(|| e.run.dir.path.clone())))?;
        let Some(dir) = dir else { return Ok(Vec::new()) };
        Ok(crate::spsa::read_history(&dir)?.into_iter().skip(skip).collect())
    }

    /// A running game's moves and clocks so far.
    pub fn live(&self, id: &str, pair: u32, game: u8) -> Result<LiveGame, TourneyError> {
        self.with(id, |e| {
            e.run
                .live((pair, game))
                .ok_or_else(|| TourneyError::Refused(format!("{id} isn't playing game {pair}.{game}")))
        })
    }

    /// A finished game's record.
    pub fn record(&self, id: &str, pair: u32, game: u8) -> Result<String, TourneyError> {
        let path = self.with(id, |e| {
            let entry = e.run.entries().into_iter().find(|g| g.pair == pair && g.game == game);
            let entry =
                entry.ok_or_else(|| TourneyError::Refused(format!("{id} has no game {pair}.{game}")))?;
            Ok(e.run.dir.path.join(entry.record))
        })?;
        std::fs::read_to_string(&path).map_err(TourneyError::io(&path))
    }

    /// Starts no more of the test's games until it's resumed.
    pub fn pause(&self, id: &str) -> Result<TestInfo, TourneyError> {
        self.update(id, |e| {
            if e.saved.status != TestStatus::Active {
                return Err(TourneyError::Refused(format!("{id} isn't active")));
            }
            e.run.hold.stop();
            e.saved.status = TestStatus::Paused;
            Ok(())
        })
    }

    /// Makes a paused or finished test active again. A test the SPRT
    /// decided can't be; one halted by failing engines can, once they're
    /// fixed (the failure counts start again).
    pub fn resume(&self, id: &str) -> Result<TestInfo, TourneyError> {
        self.refuse_if_closing()?;
        self.update(id, |e| {
            if e.saved.status == TestStatus::Active {
                return Err(TourneyError::Refused(format!("{id} is already active")));
            }
            if e.run.unlikely_to_decide() {
                return Err(TourneyError::Refused(format!(
                    "{id} is unlikely to decide within its max pairs; raise them"
                )));
            }
            if !e.run.clear_halt() {
                return Err(TourneyError::Refused(format!("the SPRT decided {id}; it can't be resumed")));
            }
            e.run.hold.clear();
            e.saved.status = TestStatus::Active;
            e.saved.end = None;
            e.settle();
            if e.saved.end == Some(RunEnd::MaxPairs) {
                let raise = if e.run.dir.spec.spsa.is_some() { "" } else { "; raise its max pairs" };
                return Err(TourneyError::Refused(format!("{id} has played all its pairs{raise}")));
            }
            Ok(())
        })
    }

    /// Ends the test: no more games start. Running games finish and are
    /// logged. It can be resumed.
    pub fn stop(&self, id: &str) -> Result<TestInfo, TourneyError> {
        self.update(id, |e| {
            if e.saved.status == TestStatus::Finished {
                return Err(TourneyError::Refused(format!("{id} has finished already")));
            }
            e.run.hold.stop();
            e.saved.status = TestStatus::Finished;
            e.saved.end = Some(RunEnd::Stopped);
            Ok(())
        })
    }

    pub fn set_priority(&self, id: &str, priority: i32) -> Result<TestInfo, TourneyError> {
        self.update(id, |e| {
            e.saved.priority = priority;
            Ok(())
        })
    }

    /// Changes how many of the test's games may run at once. Games
    /// beyond a lowered limit finish; a raised one takes free slots.
    pub fn set_concurrency(&self, id: &str, concurrency: u32) -> Result<TestInfo, TourneyError> {
        if concurrency == 0 {
            return Err(TourneyError::Refused("games at once must be at least 1".into()));
        }
        self.update(id, |e| {
            e.run.set_concurrency(concurrency);
            e.saved.concurrency = Some(concurrency);
            Ok(())
        })
    }

    /// Changes the test's pair limit (None: until the SPRT decides). A
    /// test that had played all its pairs carries on if the limit rises.
    pub fn set_max_pairs(&self, id: &str, max_pairs: Option<u32>) -> Result<TestInfo, TourneyError> {
        self.update(id, |e| {
            if e.run.dir.spec.spsa.is_some() {
                return Err(TourneyError::Refused(format!("{id} is an SPSA run; it plays its iterations")));
            }
            if max_pairs.is_none() && e.run.dir.spec.sprt.is_none() && !e.run.dir.spec.is_pool() {
                return Err(TourneyError::Refused(format!("{id} has no SPRT, so it needs a pair limit")));
            }
            if max_pairs == Some(0) {
                return Err(TourneyError::Refused("max pairs must be at least 1".into()));
            }
            e.run.set_max_pairs(max_pairs);
            e.saved.max_pairs = max_pairs;
            e.reconsider();
            Ok(())
        })
    }

    /// Changes the SPRT's chance of deciding within the pair limit below
    /// which the test ends undecided (None: play on to the limit). A test
    /// that ended undecided carries on if it's now above it; an active one
    /// now below it ends.
    pub fn set_undecided_below(&self, id: &str, cutoff: Option<f64>) -> Result<TestInfo, TourneyError> {
        self.update(id, |e| {
            if e.run.dir.spec.sprt.is_none() {
                return Err(TourneyError::Refused(format!("{id} has no SPRT")));
            }
            if cutoff.is_some_and(|c| !(0.0 < c && c < 1.0)) {
                return Err(TourneyError::Refused("the chance must be between 0 and 1".into()));
            }
            e.run.set_undecided_below(cutoff);
            e.saved.undecided_below = Some(cutoff.unwrap_or(0.0));
            e.reconsider();
            Ok(())
        })
    }

    /// Adds an engine to a pool, snapshotted as submitting does (its
    /// program must be an absolute path). It plays from the next pair.
    pub fn add_engine(&self, id: &str, engine: EngineSpec) -> Result<TestInfo, TourneyError> {
        self.refuse_if_closing()?;
        if !engine.program.is_absolute() {
            return Err(TourneyError::Refused(format!(
                "{}'s program must be an absolute path, not {}",
                engine.name,
                engine.program.display()
            )));
        }
        if engine.name.trim().is_empty() {
            return Err(TourneyError::Refused("the engine needs a name".into()));
        }
        let run = self.with(id, |e| Ok(e.run.clone()))?;
        run.add_engine(engine, &self.0.data_dir)?;
        self.update(id, |_| Ok(()))
    }

    /// Retires a pool's engine (it plays no new pairs; its games still
    /// count), or brings it back.
    pub fn set_engine_retired(&self, id: &str, name: &str, retired: bool) -> Result<TestInfo, TourneyError> {
        self.update(id, |e| e.run.set_engine_retired(name, retired))
    }

    /// Takes a test out of the queue, zipping its directory into
    /// `archive/<id>.zip` first if `archive` (that file is returned), and
    /// deletes its directory. The test must not be active or have games
    /// running. Slots holding its engines let them go first.
    pub async fn remove(&self, id: &str, archive: bool) -> Result<Option<PathBuf>, TourneyError> {
        let entry = {
            let mut tests = self.0.tests.lock().unwrap();
            let i =
                tests.iter().position(|e| e.id == id).ok_or_else(|| TourneyError::NoTest(id.to_string()))?;
            let e = &tests[i];
            if e.saved.status == TestStatus::Active {
                return Err(TourneyError::Refused(format!("{id} is active; stop or pause it first")));
            }
            if e.run.busy() > 0 {
                return Err(TourneyError::Refused(format!("{id} still has games running")));
            }
            tests.remove(i)
        };
        entry.run.retire();
        self.0.wake.notify_waiters();
        // The slots drop the test once its engines have quit; then its
        // files are free (Windows can't delete a running program or an
        // open log).
        let deadline = Instant::now() + RETIRE_WAIT;
        while Arc::strong_count(&entry.run) > 1 && Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let path = entry.run.dir.path.clone();
        drop(entry);
        let data_dir = self.0.data_dir.clone();
        let test = id.to_string();
        let result = tokio::task::spawn_blocking(move || -> Result<Option<PathBuf>, TourneyError> {
            let zip = if archive { Some(zip_test(&path, &data_dir.join(ARCHIVE_DIR), &test)?) } else { None };
            std::fs::remove_dir_all(&path).map_err(TourneyError::io(&path))?;
            Ok(zip)
        })
        .await
        .expect("removing a test doesn't panic");
        match result {
            Ok(zip) => {
                let _ =
                    self.0.events.send(DaemonEvent::TestRemoved { test: id.into(), archive: zip.clone() });
                Ok(zip)
            }
            Err(e) => {
                // Put back what's left, if it still opens.
                let path = self.0.data_dir.join("tests").join(id);
                if let Ok(entry) = load_entry(&self.0, &path) {
                    let info = entry.info();
                    let mut tests = self.0.tests.lock().unwrap();
                    tests.push(entry);
                    tests.sort_by(|a, b| a.id.cmp(&b.id));
                    drop(tests);
                    self.changed(&info);
                }
                Err(e)
            }
        }
    }

    /// The archived tests, newest first.
    pub fn archived(&self) -> Vec<ArchivedTest> {
        let dir = self.0.data_dir.join(ARCHIVE_DIR);
        let mut list: Vec<ArchivedTest> = std::fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .filter_map(|d| {
                let d = d.ok()?;
                let file = d.file_name().to_str()?.to_string();
                if !file.ends_with(".zip") {
                    return None;
                }
                let meta = d.metadata().ok()?;
                let archived = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_secs());
                let (test, error) = match read_archived(&d.path()) {
                    Ok(t) => (Some(t), None),
                    Err(e) => (None, Some(e.to_string())),
                };
                Some(ArchivedTest { file, size: meta.len(), archived, test, error })
            })
            .collect();
        list.sort_by(|a, b| b.archived.cmp(&a.archived).then_with(|| b.file.cmp(&a.file)));
        list
    }

    /// Unzips archived test `file` back into the queue, as it was when
    /// archived (an active test can't be archived, so it comes back
    /// stopped or paused), and removes the zip.
    pub fn restore(&self, file: &str) -> Result<TestInfo, TourneyError> {
        self.refuse_if_closing()?;
        let zip_path = self.archive_path(file)?;
        let tests_dir = self.0.data_dir.join("tests");
        let id = unzip_test(&zip_path, &tests_dir)?;
        let path = tests_dir.join(&id);
        let entry = match load_entry(&self.0, &path) {
            Ok(e) => e,
            Err(e) => {
                let _ = std::fs::remove_dir_all(&path);
                return Err(e);
            }
        };
        let info = entry.info();
        {
            let mut tests = self.0.tests.lock().unwrap();
            tests.push(entry);
            tests.sort_by(|a, b| a.id.cmp(&b.id));
        }
        // The test is back either way; a zip left behind can be deleted.
        if let Err(e) = std::fs::remove_file(&zip_path) {
            eprintln!("{}: {e}", zip_path.display());
        }
        self.changed(&info);
        Ok(info)
    }

    /// Deletes archived test `file`.
    pub fn delete_archived(&self, file: &str) -> Result<(), TourneyError> {
        let path = self.archive_path(file)?;
        std::fs::remove_file(&path).map_err(TourneyError::io(&path))
    }

    /// The path of archived test `file`, a zip's file name in `archive/`.
    fn archive_path(&self, file: &str) -> Result<PathBuf, TourneyError> {
        let plain = !file.starts_with('.') && !file.contains(['/', '\\']) && file.ends_with(".zip");
        let path = self.0.data_dir.join(ARCHIVE_DIR).join(file);
        if !plain || !path.is_file() {
            return Err(TourneyError::NoTest(file.to_string()));
        }
        Ok(path)
    }

    /// Sets the number of slots. Extra slots stop after their game.
    pub fn set_slots(&self, slots: u32) {
        let mut s = self.0.slots.lock().unwrap();
        s.target = slots;
        if !self.0.closing.load(Ordering::SeqCst) {
            for i in 0..slots as usize {
                if s.alive.len() <= i {
                    s.alive.push(false);
                }
                if !s.alive[i] {
                    s.alive[i] = true;
                    let inner = self.0.clone();
                    s.handles.push(tokio::spawn(async move {
                        loop {
                            let _ = play_slot(inner.clone(), i).await;
                            // A cancelled shutdown (or more slots again) can
                            // come after the slot decided to stop: then it
                            // carries on, since it's still counted alive.
                            let mut s = inner.slots.lock().unwrap();
                            if !inner.closing.load(Ordering::SeqCst) && i < s.target as usize {
                                continue;
                            }
                            s.alive[i] = false;
                            break;
                        }
                    }));
                }
            }
        }
        drop(s);
        let _ = self.0.events.send(DaemonEvent::SlotsChanged { slots });
        self.0.wake.notify_waiters();
    }

    pub fn status(&self) -> DaemonStatus {
        let tests = self.0.tests.lock().unwrap();
        let mut running = Vec::new();
        for e in tests.iter() {
            for (pair, game) in e.run.running() {
                let names = e.run.pair_names(pair);
                let gold = GameEntry::gold_of(game);
                running.push(RunningGame {
                    test: e.id.clone(),
                    pair,
                    game,
                    gold: names[gold].clone(),
                    silver: names[1 - gold].clone(),
                });
            }
        }
        DaemonStatus {
            slots: self.0.slots.lock().unwrap().target,
            running,
            tests: tests.len() as u32,
            data_dir: self.0.data_dir.clone(),
            pid: std::process::id(),
            closing: self.0.closing.load(Ordering::SeqCst),
            memory_budget: self.0.engines.budget(),
            engine_memory: self.0.engines.total(),
            engines: self.0.engines.count(),
        }
    }

    /// Starts no more games. Running games finish and are logged, unless
    /// `now`: then they're abandoned (and played again after a restart).
    /// [`Daemon::closed`] says when the slots are done.
    pub fn shutdown(&self, now: bool) {
        self.0.closing.store(true, Ordering::SeqCst);
        if now {
            let mut s = self.0.slots.lock().unwrap();
            s.abandoned = true;
            for h in &s.handles {
                h.abort();
            }
        }
        let _ = self.0.events.send(DaemonEvent::ShuttingDown { now });
        self.0.wake.notify_waiters();
    }

    /// Calls off a shutdown that's letting the running games finish: the
    /// daemon goes on as before. Refused once the games were abandoned
    /// (`now`) or the daemon has shut down.
    pub fn cancel_shutdown(&self) -> Result<(), TourneyError> {
        let s = self.0.slots.lock().unwrap();
        if s.closed {
            return Err(TourneyError::Refused("the daemon has shut down".into()));
        }
        if s.abandoned {
            return Err(TourneyError::Refused("the daemon is shutting down now".into()));
        }
        if !self.0.closing.swap(false, Ordering::SeqCst) {
            return Ok(());
        }
        let target = s.target;
        drop(s);
        // Slots that already stopped start again.
        self.set_slots(target);
        let _ = self.0.events.send(DaemonEvent::ShutdownCancelled);
        Ok(())
    }

    /// Waits until the daemon has shut down and every slot has ended.
    pub async fn closed(&self) {
        loop {
            loop {
                let notified = self.0.wake.notified();
                tokio::pin!(notified);
                notified.as_mut().enable();
                if self.0.closing.load(Ordering::SeqCst) {
                    break;
                }
                notified.await;
            }
            loop {
                let handles: Vec<JoinHandle<()>> = {
                    let mut s = self.0.slots.lock().unwrap();
                    if s.handles.is_empty() {
                        // Decided under the lock, so a cancel either comes
                        // first or is refused.
                        if self.0.closing.load(Ordering::SeqCst) {
                            s.closed = true;
                            return;
                        }
                        break;
                    }
                    std::mem::take(&mut s.handles)
                };
                for h in handles {
                    let _ = h.await;
                }
            }
            // Cancelled with no slots to wait for: wait for the next shutdown.
        }
    }

    fn refuse_if_closing(&self) -> Result<(), TourneyError> {
        if self.0.closing.load(Ordering::SeqCst) {
            return Err(TourneyError::Refused("the daemon is shutting down".into()));
        }
        Ok(())
    }

    fn with<T>(
        &self,
        id: &str,
        f: impl FnOnce(&Entry) -> Result<T, TourneyError>,
    ) -> Result<T, TourneyError> {
        let tests = self.0.tests.lock().unwrap();
        let entry = tests.iter().find(|e| e.id == id).ok_or_else(|| TourneyError::NoTest(id.to_string()))?;
        f(entry)
    }

    /// Changes a test, saves its state and tells clients.
    fn update(
        &self,
        id: &str,
        f: impl FnOnce(&mut Entry) -> Result<(), TourneyError>,
    ) -> Result<TestInfo, TourneyError> {
        let mut tests = self.0.tests.lock().unwrap();
        let entry =
            tests.iter_mut().find(|e| e.id == id).ok_or_else(|| TourneyError::NoTest(id.to_string()))?;
        let before = entry.saved.clone();
        if let Err(e) = f(entry) {
            entry.saved = before;
            return Err(e);
        }
        entry.save()?;
        let parts = entry.info_parts();
        drop(tests);
        let info = parts.finish();
        self.changed(&info);
        Ok(info)
    }

    fn changed(&self, info: &TestInfo) {
        let _ = self.0.events.send(DaemonEvent::TestChanged { test: Box::new(info.clone()) });
        self.0.wake.notify_waiters();
    }
}

/// Zips test `id`'s directory `dir` into `archive_dir/<id>.zip`, its files
/// under `<id>/`, and returns the zip's path. Nothing is left behind if
/// it fails.
fn zip_test(dir: &Path, archive_dir: &Path, id: &str) -> Result<PathBuf, TourneyError> {
    std::fs::create_dir_all(archive_dir).map_err(TourneyError::io(archive_dir))?;
    let mut zip_path = archive_dir.join(format!("{id}.zip"));
    let mut n = 2;
    while zip_path.exists() {
        zip_path = archive_dir.join(format!("{id}-{n}.zip"));
        n += 1;
    }
    let tmp = zip_path.with_extension("zip.tmp");
    let result = (|| {
        let file = std::fs::File::create(&tmp).map_err(TourneyError::io(&tmp))?;
        let mut zip = zip::ZipWriter::new(std::io::BufWriter::new(file));
        add_to_zip(&mut zip, dir, id)?;
        let zip_err = |e: zip::result::ZipError| TourneyError::Io { path: tmp.clone(), source: e.into() };
        let mut out = zip.finish().map_err(zip_err)?;
        std::io::Write::flush(&mut out).map_err(TourneyError::io(&tmp))?;
        drop(out);
        std::fs::rename(&tmp, &zip_path).map_err(TourneyError::io(&zip_path))
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result.map(|()| zip_path)
}

/// The top directory every entry of an archived test's zip is under: the
/// test's id.
fn zip_id<R: std::io::Read + std::io::Seek>(
    zip: &mut zip::ZipArchive<R>,
    zip_path: &Path,
) -> Result<String, TourneyError> {
    let bad = |message: String| TourneyError::Parse { path: zip_path.to_path_buf(), message };
    let mut id: Option<String> = None;
    for i in 0..zip.len() {
        let entry = zip.by_index(i).map_err(|e| bad(e.to_string()))?;
        let name = entry.enclosed_name().ok_or_else(|| bad(format!("unsafe path {:?}", entry.name())))?;
        let top = match name.components().next() {
            Some(std::path::Component::Normal(c)) => c.to_string_lossy().into_owned(),
            _ => return Err(bad(format!("unsafe path {:?}", entry.name()))),
        };
        match &id {
            Some(id) if *id != top => return Err(bad("isn't one test's directory".into())),
            Some(_) => {}
            None => id = Some(top),
        }
    }
    let id = id.ok_or_else(|| bad("is empty".into()))?;
    if id.starts_with('.') {
        return Err(bad(format!("unsafe test id {id:?}")));
    }
    Ok(id)
}

fn read_zip_text<R: std::io::Read + std::io::Seek>(
    zip: &mut zip::ZipArchive<R>,
    zip_path: &Path,
    name: &str,
) -> Result<Option<String>, TourneyError> {
    let mut entry = match zip.by_name(name) {
        Ok(e) => e,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(e) => return Err(TourneyError::Parse { path: zip_path.join(name), message: e.to_string() }),
    };
    let mut text = String::new();
    std::io::Read::read_to_string(&mut entry, &mut text).map_err(TourneyError::io(zip_path.join(name)))?;
    Ok(Some(text))
}

/// An archived test as it was, from its zip.
fn read_archived(zip_path: &Path) -> Result<TestInfo, TourneyError> {
    let file = std::fs::File::open(zip_path).map_err(TourneyError::io(zip_path))?;
    let mut zip = zip::ZipArchive::new(std::io::BufReader::new(file))
        .map_err(|e| TourneyError::Parse { path: zip_path.to_path_buf(), message: e.to_string() })?;
    let id = zip_id(&mut zip, zip_path)?;
    let spec_name = format!("{id}/{}", crate::testdir::SPEC_FILE);
    let text = read_zip_text(&mut zip, zip_path, &spec_name)?.ok_or_else(|| TourneyError::Parse {
        path: zip_path.to_path_buf(),
        message: format!("has no {spec_name}"),
    })?;
    let spec = TestSpec::parse(&text)
        .map_err(|message| TourneyError::Parse { path: zip_path.join(&spec_name), message })?;
    let state_name = format!("{id}/{STATE_FILE}");
    let saved: Option<SavedState> =
        match read_zip_text(&mut zip, zip_path, &state_name)? {
            Some(text) => Some(serde_json::from_str(&text).map_err(|e| TourneyError::Parse {
                path: zip_path.join(&state_name),
                message: e.to_string(),
            })?),
            None => None,
        };
    let results_name = format!("{id}/{}", crate::testdir::RESULTS_FILE);
    let entries = match read_zip_text(&mut zip, zip_path, &results_name)? {
        Some(text) => parse_results(&text, &zip_path.join(&results_name))?,
        None => Vec::new(),
    };
    let spsa = match &spec.spsa {
        Some(sp) => {
            let name = format!("{id}/{}", crate::spsa::HISTORY_FILE);
            let text = read_zip_text(&mut zip, zip_path, &name)?.unwrap_or_default();
            Some(crate::spsa::info_from_history(sp, &text, &zip_path.join(&name))?)
        }
        None => None,
    };
    Ok(TestInfo {
        id,
        name: spec.name.clone(),
        engines: spec.side_names(),
        time_control: spec.time_control.clone(),
        status: saved.as_ref().map_or(TestStatus::Finished, |s| s.status),
        priority: saved.as_ref().map_or(0, |s| s.priority),
        max_pairs: saved.as_ref().map_or(spec.pair_limit(), |s| s.max_pairs),
        sprt: spec.sprt,
        concurrency: saved.as_ref().and_then(|s| s.concurrency).unwrap_or(spec.concurrency),
        running: Vec::new(),
        running_engines: Vec::new(),
        end: saved.as_ref().and_then(|s| s.end.clone()),
        submitted: saved.as_ref().map_or(0, |s| s.submitted),
        summary: crate::stats::summarize(spec.sprt, &entries),
        undecided_below: match &saved {
            Some(s) => s.undecided_below(&spec),
            None => spec.sprt.and_then(|s| s.undecided_below),
        },
        decide_chance: None,
        projection: None,
        spsa,
        pool: spec.pool.as_ref().map(|p| crate::pool::pool_info(p, &spec.engines, &entries)),
        path: zip_path.to_path_buf(),
    }
    .with_decide_chance())
}

/// Unzips an archived test into `tests_dir/<id>/` and returns its id.
/// Refuses if a test with that id is there already; leaves nothing
/// behind if it fails.
fn unzip_test(zip_path: &Path, tests_dir: &Path) -> Result<String, TourneyError> {
    let file = std::fs::File::open(zip_path).map_err(TourneyError::io(zip_path))?;
    let mut zip = zip::ZipArchive::new(std::io::BufReader::new(file))
        .map_err(|e| TourneyError::Parse { path: zip_path.to_path_buf(), message: e.to_string() })?;
    let id = zip_id(&mut zip, zip_path)?;
    let dir = tests_dir.join(&id);
    // Claims the id, as submitting does.
    match std::fs::create_dir(&dir) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(TourneyError::Refused(format!("a test {id} is in the queue already")));
        }
        Err(e) => return Err(TourneyError::io(&dir)(e)),
    }
    let result = (|| {
        for i in 0..zip.len() {
            let mut entry = zip
                .by_index(i)
                .map_err(|e| TourneyError::Parse { path: zip_path.to_path_buf(), message: e.to_string() })?;
            let name = entry.enclosed_name().expect("checked by zip_id");
            let to = tests_dir.join(name);
            if entry.is_dir() {
                std::fs::create_dir_all(&to).map_err(TourneyError::io(&to))?;
                continue;
            }
            if let Some(parent) = to.parent() {
                std::fs::create_dir_all(parent).map_err(TourneyError::io(parent))?;
            }
            let mut out = std::fs::File::create(&to).map_err(TourneyError::io(&to))?;
            std::io::copy(&mut entry, &mut out).map_err(TourneyError::io(&to))?;
            #[cfg(unix)]
            if let Some(mode) = entry.unix_mode() {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&to, std::fs::Permissions::from_mode(mode & 0o7777))
                    .map_err(TourneyError::io(&to))?;
            }
        }
        if !dir.join(crate::testdir::SPEC_FILE).is_file() {
            return Err(TourneyError::Parse {
                path: zip_path.to_path_buf(),
                message: format!("has no {id}/{}", crate::testdir::SPEC_FILE),
            });
        }
        Ok(())
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_dir_all(&dir);
        return Err(e);
    }
    Ok(id)
}

fn add_to_zip<W: std::io::Write + std::io::Seek>(
    zip: &mut zip::ZipWriter<W>,
    dir: &Path,
    prefix: &str,
) -> Result<(), TourneyError> {
    use zip::write::SimpleFileOptions;
    let zip_err = |path: &Path| {
        let path = path.to_path_buf();
        move |e: zip::result::ZipError| TourneyError::Io { path, source: e.into() }
    };
    let mut items: Vec<std::fs::DirEntry> = std::fs::read_dir(dir)
        .map_err(TourneyError::io(dir))?
        .collect::<Result<_, _>>()
        .map_err(TourneyError::io(dir))?;
    items.sort_by_key(|d| d.file_name());
    for item in items {
        let path = item.path();
        let name = format!("{prefix}/{}", item.file_name().to_string_lossy());
        let meta = std::fs::metadata(&path).map_err(TourneyError::io(&path))?;
        let options = SimpleFileOptions::default().large_file(meta.len() >= 1 << 32);
        #[cfg(unix)]
        let options = options.unix_permissions(std::os::unix::fs::PermissionsExt::mode(&meta.permissions()));
        if meta.is_dir() {
            zip.add_directory(&name, options).map_err(zip_err(&path))?;
            add_to_zip(zip, &path, &name)?;
        } else {
            zip.start_file(&name, options).map_err(zip_err(&path))?;
            let mut file = std::fs::File::open(&path).map_err(TourneyError::io(&path))?;
            std::io::copy(&mut file, zip).map_err(TourneyError::io(&path))?;
        }
    }
    Ok(())
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn load_entry(inner: &Arc<Inner>, path: &Path) -> Result<Entry, TourneyError> {
    let state_path = path.join(STATE_FILE);
    let saved = match std::fs::read_to_string(&state_path) {
        Ok(text) => serde_json::from_str(&text)
            .map_err(|e| TourneyError::Parse { path: state_path.clone(), message: e.to_string() })?,
        // A test made by `new` and put here by hand.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => SavedState {
            status: TestStatus::Active,
            priority: 0,
            max_pairs: TestDir::open(path)?.spec.pair_limit(),
            concurrency: None,
            undecided_below: None,
            submitted: now(),
            end: None,
        },
        Err(e) => return Err(TourneyError::io(&state_path)(e)),
    };
    let mut entry = load_entry_with(inner, path, saved)?;
    let before = entry.saved.clone();
    entry.settle();
    if entry.saved != before {
        entry.save()?;
    }
    Ok(entry)
}

fn load_entry_with(inner: &Arc<Inner>, path: &Path, saved: SavedState) -> Result<Entry, TourneyError> {
    let dir = TestDir::open(path)?;
    let id = path.file_name().expect("a test's directory has a name").to_string_lossy().into_owned();
    let concurrency = saved.concurrency.unwrap_or(dir.spec.concurrency);
    let run = TestRun::open(dir, concurrency, event_fn(inner, &id))?;
    run.set_max_pairs(saved.max_pairs);
    run.set_undecided_below(saved.undecided_below(&run.dir.spec));
    if saved.status != TestStatus::Active {
        run.hold.stop();
    }
    Ok(Entry { id, run: Arc::new(run), saved, projection: Arc::default() })
}

fn event_fn(inner: &Arc<Inner>, id: &str) -> EventFn {
    let events = inner.events.clone();
    let output = inner.output.clone();
    let test = id.to_string();
    Box::new(move |e| {
        let test = test.clone();
        let event = match e {
            RunEvent::EngineOutput { pair, game, side, line } => {
                // Nobody may be listening.
                let _ = output.send(GameOutput { test, pair, game, side, line });
                return;
            }
            RunEvent::GameStarted { pair, game } => DaemonEvent::GameStarted { test, pair, game },
            RunEvent::MovePlayed { pair, game, ply, notation, used, reserves } => DaemonEvent::MovePlayed {
                test,
                pair,
                game,
                ply,
                notation,
                used_ms: used.as_millis() as u64,
                reserves_ms: reserves.map(|r| r.map(|d| d.as_millis() as u64)),
            },
            RunEvent::GameFinished { entry, .. } => DaemonEvent::GameFinished { test, entry },
            RunEvent::EngineRestarted { engine, name, reason } => {
                DaemonEvent::EngineRestarted { test, engine, name, reason }
            }
            RunEvent::EngineFailedToStart { engine, name, error } => {
                DaemonEvent::EngineFailedToStart { test, engine, name, error }
            }
            RunEvent::EngineRetired { name, reason } => DaemonEvent::EngineRetired { test, name, reason },
        };
        let _ = events.send(event);
    })
}

impl Inner {
    /// The next game for a slot: from the active test with the highest
    /// priority that has one, preferring the slot's current test, then the
    /// test with the fewest games running.
    fn pick(&self, current: Option<&Arc<TestRun>>) -> Option<(Arc<TestRun>, (u32, u8))> {
        let tests = self.tests.lock().unwrap();
        let mut order: Vec<&Entry> = tests.iter().filter(|e| e.saved.status == TestStatus::Active).collect();
        let is_current = |e: &Entry| current.is_some_and(|c| Arc::ptr_eq(c, &e.run));
        order.sort_by_key(|e| (Reverse(e.saved.priority), !is_current(e), e.run.busy(), e.id.clone()));
        order.into_iter().find_map(|e| e.run.next_game().map(|g| (e.run.clone(), g)))
    }
}

impl Picker for Inner {
    async fn next(&self, slot: usize, current: Option<&Arc<TestRun>>) -> Option<(Arc<TestRun>, (u32, u8))> {
        loop {
            let notified = self.wake.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.closing.load(Ordering::SeqCst) || slot >= self.slots.lock().unwrap().target as usize {
                return None;
            }
            if let Some(next) = self.pick(current) {
                return Some(next);
            }
            notified.await;
        }
    }

    fn after_game(&self, run: &Arc<TestRun>) {
        let mut tests = self.tests.lock().unwrap();
        let Some(entry) = tests.iter_mut().find(|e| Arc::ptr_eq(&e.run, run)) else { return };
        let before = entry.saved.clone();
        entry.settle();
        if entry.saved != before
            && let Err(e) = entry.save()
        {
            eprintln!("{}: {e}", entry.id);
        }
        let parts = entry.info_parts();
        drop(tests);
        let info = parts.finish();
        let _ = self.events.send(DaemonEvent::TestChanged { test: Box::new(info) });
        self.wake.notify_waiters();
    }

    fn failed(&self, run: &Arc<TestRun>, error: TourneyError) -> Option<TourneyError> {
        eprintln!("{}: {error}", run.dir.path.display());
        None
    }

    fn engines(&self) -> &Engines {
        &self.engines
    }

    fn stopping(&self, slot: usize) -> bool {
        self.closing.load(Ordering::SeqCst) || slot >= self.slots.lock().unwrap().target as usize
    }
}
