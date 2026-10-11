//! The daemon's queue: tests sharing a number of game slots.
//!
//! Tests live in `<data dir>/tests/<id>/`, each a [`TestDir`] plus
//! `state.json` (its [`TestStatus`], priority, pair limit and end), so the
//! queue survives a restart: tests that were active carry on. A free slot
//! takes a game from the active test with the highest priority, staying on
//! the test it holds engines for when that's among the best, and otherwise
//! the one with the fewest games running. A test's `concurrency` caps its
//! games at once.
//!
//! This is the daemon without its HTTP API (`server`), so it can be driven
//! and tested directly.

use std::cmp::Reverse;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tokio::sync::{Notify, broadcast};
use tokio::task::JoinHandle;

use crate::error::TourneyError;
use crate::results::GameEntry;
use crate::runner::{EventFn, Picker, RunEnd, RunEvent, TestRun, play_slot};
use crate::spec::{SprtSpec, TestSpec};
use crate::stats::Summary;
use crate::testdir::{TestDir, slug};

/// Overrides the data directory.
pub const DATA_DIR_ENV: &str = "HOWDAH_TOURNEY_DIR";

const STATE_FILE: &str = "state.json";

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
    /// Seconds since the Unix epoch.
    submitted: u64,
    end: Option<RunEnd>,
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
    pub end: Option<RunEnd>,
    pub submitted: u64,
    pub summary: Summary,
    /// The test's directory.
    pub path: PathBuf,
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
    GameFinished {
        test: String,
        entry: GameEntry,
    },
    EngineRestarted {
        test: String,
        engine: usize,
        reason: String,
    },
    EngineFailedToStart {
        test: String,
        engine: usize,
        error: String,
    },
    SlotsChanged {
        slots: u32,
    },
    /// No more games start; with `now`, running games are abandoned.
    ShuttingDown {
        now: bool,
    },
}

impl DaemonEvent {
    /// The event's `type`, as the server names its server-sent events.
    pub fn name(&self) -> &'static str {
        match self {
            DaemonEvent::TestChanged { .. } => "testChanged",
            DaemonEvent::GameStarted { .. } => "gameStarted",
            DaemonEvent::GameFinished { .. } => "gameFinished",
            DaemonEvent::EngineRestarted { .. } => "engineRestarted",
            DaemonEvent::EngineFailedToStart { .. } => "engineFailedToStart",
            DaemonEvent::SlotsChanged { .. } => "slotsChanged",
            DaemonEvent::ShuttingDown { .. } => "shuttingDown",
        }
    }
}

struct Entry {
    id: String,
    run: Arc<TestRun>,
    saved: SavedState,
}

impl Entry {
    fn info(&self) -> TestInfo {
        let spec = &self.run.dir.spec;
        TestInfo {
            id: self.id.clone(),
            name: spec.name.clone(),
            engines: [spec.engines[0].name.clone(), spec.engines[1].name.clone()],
            time_control: spec.time_control.clone(),
            status: self.saved.status,
            priority: self.saved.priority,
            max_pairs: self.saved.max_pairs,
            sprt: spec.sprt,
            concurrency: spec.concurrency,
            running: self.run.running(),
            end: self.saved.end.clone(),
            submitted: self.saved.submitted,
            summary: self.run.summary(),
            path: self.run.dir.path.clone(),
        }
    }

    fn save(&self) -> Result<(), TourneyError> {
        let path = self.run.dir.path.join(STATE_FILE);
        let tmp = path.with_extension("json.tmp");
        let json = serde_json::to_string_pretty(&self.saved).expect("state always serializes");
        std::fs::write(&tmp, json).map_err(TourneyError::io(&tmp))?;
        std::fs::rename(&tmp, &path).map_err(TourneyError::io(&path))
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
}

struct Inner {
    data_dir: PathBuf,
    tests: Mutex<Vec<Entry>>,
    slots: Mutex<Slots>,
    /// Wakes idle slots when there may be a game for them.
    wake: Notify,
    events: broadcast::Sender<DaemonEvent>,
    closing: AtomicBool,
}

/// The queue and its slots. Cloning shares it.
#[derive(Clone)]
pub struct Daemon(Arc<Inner>);

impl Daemon {
    /// Loads the tests in `data_dir` and starts `slots` slots. Active tests
    /// carry on where they were.
    pub fn start(data_dir: &Path, slots: u32) -> Result<Daemon, TourneyError> {
        let tests_dir = data_dir.join("tests");
        std::fs::create_dir_all(&tests_dir).map_err(TourneyError::io(&tests_dir))?;
        let (events, _) = broadcast::channel(1024);
        let inner = Arc::new(Inner {
            data_dir: data_dir.to_path_buf(),
            tests: Mutex::new(Vec::new()),
            slots: Mutex::new(Slots { target: 0, handles: Vec::new(), alive: Vec::new() }),
            wake: Notify::new(),
            events,
            closing: AtomicBool::new(false),
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
            let mut next =
                tests.iter().filter_map(|e| e.id.split('-').next()?.parse::<u32>().ok()).max().unwrap_or(0)
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
        let max_pairs = spec.max_pairs;
        let path = tests_dir.join(&id);
        TestDir::create(spec, &self.0.data_dir, &path)?;
        let saved =
            SavedState { status: TestStatus::Active, priority, max_pairs, submitted: now(), end: None };
        let entry = load_entry_with(&self.0, &path, saved)?;
        entry.save()?;
        let info = entry.info();
        self.0.tests.lock().unwrap().push(entry);
        self.changed(&info);
        Ok(info)
    }

    pub fn list(&self) -> Vec<TestInfo> {
        self.0.tests.lock().unwrap().iter().map(Entry::info).collect()
    }

    pub fn info(&self, id: &str) -> Result<TestInfo, TourneyError> {
        self.with(id, |e| Ok(e.info()))
    }

    pub fn games(&self, id: &str) -> Result<Vec<GameEntry>, TourneyError> {
        self.with(id, |e| Ok(e.run.entries()))
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
            if !e.run.clear_halt() {
                return Err(TourneyError::Refused(format!("the SPRT decided {id}; it can't be resumed")));
            }
            e.run.hold.clear();
            e.saved.status = TestStatus::Active;
            e.saved.end = None;
            e.settle();
            if e.saved.end == Some(RunEnd::MaxPairs) {
                return Err(TourneyError::Refused(format!(
                    "{id} has played all its pairs; raise its max pairs"
                )));
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

    /// Changes the test's pair limit (None: until the SPRT decides). A
    /// test that had played all its pairs carries on if the limit rises.
    pub fn set_max_pairs(&self, id: &str, max_pairs: Option<u32>) -> Result<TestInfo, TourneyError> {
        self.update(id, |e| {
            if max_pairs.is_none() && e.run.dir.spec.sprt.is_none() {
                return Err(TourneyError::Refused(format!("{id} has no SPRT, so it needs a pair limit")));
            }
            if max_pairs == Some(0) {
                return Err(TourneyError::Refused("max pairs must be at least 1".into()));
            }
            e.run.set_max_pairs(max_pairs);
            e.saved.max_pairs = max_pairs;
            if e.saved.status == TestStatus::Finished && e.saved.end == Some(RunEnd::MaxPairs) {
                e.saved.status = TestStatus::Active;
                e.saved.end = None;
                e.run.hold.clear();
            }
            e.settle();
            Ok(())
        })
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
                        let _ = play_slot(inner.clone(), i).await;
                        inner.slots.lock().unwrap().alive[i] = false;
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
            let names = &e.run.dir.spec.engines;
            for (pair, game) in e.run.running() {
                let gold = GameEntry::gold_of(game);
                running.push(RunningGame {
                    test: e.id.clone(),
                    pair,
                    game,
                    gold: names[gold].name.clone(),
                    silver: names[1 - gold].name.clone(),
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
        }
    }

    /// Starts no more games. Running games finish and are logged, unless
    /// `now`: then they're abandoned (and played again after a restart).
    /// [`Daemon::closed`] says when the slots are done.
    pub fn shutdown(&self, now: bool) {
        self.0.closing.store(true, Ordering::SeqCst);
        if now {
            for h in &self.0.slots.lock().unwrap().handles {
                h.abort();
            }
        }
        let _ = self.0.events.send(DaemonEvent::ShuttingDown { now });
        self.0.wake.notify_waiters();
    }

    /// Waits until the daemon has shut down and every slot has ended.
    pub async fn closed(&self) {
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
            let handles: Vec<JoinHandle<()>> = std::mem::take(&mut self.0.slots.lock().unwrap().handles);
            if handles.is_empty() {
                break;
            }
            for h in handles {
                let _ = h.await;
            }
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
        let info = entry.info();
        drop(tests);
        self.changed(&info);
        Ok(info)
    }

    fn changed(&self, info: &TestInfo) {
        let _ = self.0.events.send(DaemonEvent::TestChanged { test: Box::new(info.clone()) });
        self.0.wake.notify_waiters();
    }
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
            max_pairs: TestDir::open(path)?.spec.max_pairs,
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
    let concurrency = dir.spec.concurrency;
    let run = TestRun::open(dir, concurrency, event_fn(inner, &id))?;
    run.set_max_pairs(saved.max_pairs);
    if saved.status != TestStatus::Active {
        run.hold.stop();
    }
    Ok(Entry { id, run: Arc::new(run), saved })
}

fn event_fn(inner: &Arc<Inner>, id: &str) -> EventFn {
    let events = inner.events.clone();
    let test = id.to_string();
    Box::new(move |e| {
        let test = test.clone();
        let event = match e {
            RunEvent::GameStarted { pair, game } => DaemonEvent::GameStarted { test, pair, game },
            RunEvent::GameFinished { entry, .. } => DaemonEvent::GameFinished { test, entry },
            RunEvent::EngineRestarted { engine, reason } => {
                DaemonEvent::EngineRestarted { test, engine, reason }
            }
            RunEvent::EngineFailedToStart { engine, error } => {
                DaemonEvent::EngineFailedToStart { test, engine, error }
            }
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
        order.sort_by_key(|e| {
            (Reverse(e.saved.priority), !is_current(e), e.run.running().len(), e.id.clone())
        });
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
        let info = entry.info();
        drop(tests);
        let _ = self.events.send(DaemonEvent::TestChanged { test: Box::new(info) });
        self.wake.notify_waiters();
    }

    fn failed(&self, run: &Arc<TestRun>, error: TourneyError) -> Option<TourneyError> {
        eprintln!("{}: {error}", run.dir.path.display());
        None
    }
}
