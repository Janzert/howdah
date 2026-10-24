//! Playing a test's games.
//!
//! A [`TestRun`] is one test's games in play: which are done, which are
//! running, its engines' failures and its end. Slots play the games: each
//! holds a process per engine, reused from game to game while it stays on
//! the same test, and asks a picker for its next game: for [`run`] the one
//! test, for the daemon its queue. A game that ends is written as a record
//! and appended to the log before the slot moves on, so a run stopped or
//! killed at any point resumes by playing whatever isn't logged.
//!
//! Engines' memory is watched (`memory`): a slot lets its engines go when
//! it has had nothing to play for [`IDLE_KEEP`], or when the engines
//! together are near the budget; an engine that has grown too much is
//! restarted between games, and one past its limit (or the largest, when
//! all of them are past the budget) forfeits its game.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::File;
use std::future::Future;
use std::io::{BufWriter, Write};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use howdah_aei::{Direction, Engine, Info, MatchConfig, MatchEvent, MatchOutcome, play_match};
use howdah_arimaa::{
    Color, DEFAULT_SETUP_MINIMUM, Game, GameRecord, GameResult, GameTree, SETUP_MINIMUM_TAG, TimeControl,
    WinReason,
};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::UnboundedSender;
use tokio::task::JoinSet;

use crate::error::TourneyError;
use crate::memory::{self, Engines, Limits, size};
use crate::results::{GameEntry, RESOURCE_LIMIT, Restart, ResultsLog, moves_hash};
use crate::spec::{OptionValue, TestSpec};
use crate::spsa::{SpsaInfo, SpsaRun};
use crate::stats::{Decision, Summary, decide_chance, summarize};
use crate::testdir::TestDir;

/// An engine failing this many games in a row (crashing, or not starting)
/// halts the test, rather than piling up losses.
const MAX_FAILURES: u32 = 3;

/// How long to wait before trying again after an engine didn't start.
const RETRY_DELAY: Duration = Duration::from_secs(1);

/// How long a slot with nothing to play keeps its engines. Idle engines
/// still hold their memory, so they're let go unless a game comes soon.
const IDLE_KEEP: Duration = Duration::from_secs(10);

#[derive(Clone, Debug, Default)]
pub struct RunOptions {
    /// Games at once, instead of the spec's.
    pub concurrency: Option<u32>,
    /// Bytes all the engines may hold together ([`crate::default_memory_budget`]
    /// if None).
    pub memory_budget: Option<u64>,
}

/// Something an engine said while searching in a running game, as
/// `play_match` reports it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EngineLine {
    Info(Info),
    Log(String),
}

/// How many of each side's latest lines a running game keeps, for a
/// viewer joining it.
const LINES_KEPT: usize = 150;

#[derive(Clone, Debug)]
pub enum RunEvent {
    GameStarted {
        pair: u32,
        game: u8,
    },
    /// A move in a running game.
    MovePlayed {
        pair: u32,
        game: u8,
        /// The move's ply (0 is gold's setup).
        ply: usize,
        notation: String,
        used: Duration,
        /// Both reserves after the move (gold, silver), if the game has
        /// clocks.
        reserves: Option<[Duration; 2]>,
    },
    GameFinished {
        entry: GameEntry,
        summary: Box<Summary>,
    },
    /// An engine's search output in a running game.
    EngineOutput {
        pair: u32,
        game: u8,
        side: Color,
        line: EngineLine,
    },
    /// An engine was restarted after failing, timing out (it may still
    /// be searching), or using too much memory.
    EngineRestarted {
        engine: usize,
        reason: String,
    },
    EngineFailedToStart {
        engine: usize,
        error: String,
    },
}

/// Why a run ended.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "detail")]
pub enum RunEnd {
    /// The SPRT decided.
    Decided(Decision),
    /// Every pair up to `max_pairs` was played.
    MaxPairs,
    /// The SPRT was unlikely to decide within `max_pairs`: its chance of
    /// doing so fell below the spec's `undecided_below`.
    Undecided,
    /// [`StopHandle::stop`] was called; running games were finished.
    Stopped,
    /// Something went wrong that needs a person (an engine failing again
    /// and again, a file that couldn't be written).
    Halted(String),
}

/// Asks a run to start no more games. Games already running finish and
/// are logged.
#[derive(Clone, Debug, Default)]
pub struct StopHandle(Arc<AtomicBool>);

impl StopHandle {
    pub fn stop(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_stopped(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }

    pub(crate) fn clear(&self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

pub(crate) type EventFn = Box<dyn Fn(RunEvent) + Send + Sync>;

struct State {
    entries: Vec<GameEntry>,
    done: HashSet<(u32, u8)>,
    /// Games being played.
    running: Vec<(u32, u8)>,
    /// Games handed out that couldn't be played, to hand out again first.
    retry: VecDeque<(u32, u8)>,
    /// The next game in order to consider.
    cursor: (u32, u8),
    max_pairs: Option<u32>,
    /// The SPRT's chance of deciding within `max_pairs` below which the
    /// test ends undecided.
    undecided_below: Option<f64>,
    /// Games each engine has failed in a row.
    failures: [u32; 2],
    end: Option<RunEnd>,
    log: ResultsLog,
    /// The moves and clocks of the games being played.
    live: HashMap<(u32, u8), Live>,
    /// An SPSA run's parameters.
    spsa: Option<SpsaRun>,
}

/// A running game as far as it has got.
struct Live {
    moves: Vec<String>,
    used: Vec<Duration>,
    reserves: Option<[Duration; 2]>,
    /// When the side to move's turn started.
    turn_started: Instant,
    /// The engines' `id name`s (gold, silver).
    ids: [Option<String>; 2],
    /// Each side's latest search output.
    output: [VecDeque<EngineLine>; 2],
}

/// A running game as far as it has got, for a viewer joining it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveGame {
    pub pair: u32,
    pub game: u8,
    pub gold: String,
    pub silver: String,
    pub time_control: Option<String>,
    /// The `SetupMinimum` tag's value, when the setup minimum isn't
    /// arimaa.com's minute. Daemons from before it was kept played with
    /// the minute and send none.
    #[serde(default)]
    pub setup_minimum: Option<String>,
    pub moves: Vec<String>,
    /// Milliseconds each move took.
    pub used_ms: Vec<u64>,
    /// Both reserves (gold, silver) in milliseconds, as the turn being
    /// played started, if the game has clocks.
    pub reserves_ms: Option<[u64; 2]>,
    /// Milliseconds the side to move has spent on this turn.
    pub turn_elapsed_ms: u64,
    /// The engines' `id name`s (gold, silver), as they gave them.
    #[serde(default)]
    pub engine_ids: [Option<String>; 2],
    /// Each side's latest search output (gold, silver), oldest first;
    /// later lines come from the game's output stream.
    #[serde(default)]
    pub output: [Vec<EngineLine>; 2],
}

/// One test's games in play.
pub(crate) struct TestRun {
    pub(crate) dir: TestDir,
    config: MatchConfig,
    limits: Limits,
    /// Games at once at most.
    concurrency: AtomicU32,
    state: Mutex<State>,
    /// Set while the test should start no more games.
    pub(crate) hold: StopHandle,
    /// Set when the test is taken out of the daemon (archived or
    /// deleted): slots holding its engines let them go.
    retired: AtomicBool,
    retire: tokio::sync::Notify,
    on_event: EventFn,
}

impl TestRun {
    pub(crate) fn open(dir: TestDir, concurrency: u32, on_event: EventFn) -> Result<TestRun, TourneyError> {
        let entries = dir.results()?;
        std::fs::create_dir_all(dir.path.join("games")).map_err(TourneyError::io(dir.path.join("games")))?;
        if dir.spec.engine_logs {
            std::fs::create_dir_all(dir.path.join("logs"))
                .map_err(TourneyError::io(dir.path.join("logs")))?;
        }
        let log = ResultsLog::open(&dir.results_path())?;
        let end = summarize(dir.spec.sprt, &entries).decision.map(RunEnd::Decided);
        let spsa = match &dir.spec.spsa {
            Some(spec) => Some(SpsaRun::open(&dir.path, spec, &entries)?),
            None => None,
        };
        Ok(TestRun {
            config: MatchConfig { time_control: dir.spec.time_control(), ..MatchConfig::default() },
            limits: Limits::new(&dir.spec.memory),
            concurrency: AtomicU32::new(concurrency.max(1)),
            state: Mutex::new(State {
                done: entries.iter().map(|e| (e.pair, e.game)).collect(),
                entries,
                running: Vec::new(),
                retry: VecDeque::new(),
                cursor: (1, 1),
                max_pairs: dir.spec.pair_limit(),
                undecided_below: dir.spec.sprt.and_then(|s| s.undecided_below),
                failures: [0; 2],
                end,
                log,
                live: HashMap::new(),
                spsa,
            }),
            dir,
            hold: StopHandle::default(),
            retired: AtomicBool::new(false),
            retire: tokio::sync::Notify::new(),
            on_event,
        })
    }

    /// Takes the next game to play, if the test has one to start now.
    pub(crate) fn next_game(&self) -> Option<(u32, u8)> {
        let mut st = self.state.lock().unwrap();
        if st.end.is_some() || self.hold.is_stopped() || st.running.len() as u32 >= self.concurrency() {
            return None;
        }
        let g = match st.retry.pop_front() {
            Some(g) => g,
            None => loop {
                let g = st.cursor;
                if st.max_pairs.is_some_and(|m| g.0 > m) {
                    return None;
                }
                st.cursor = if g.1 == 1 { (g.0, 2) } else { (g.0 + 1, 1) };
                if !st.done.contains(&g) {
                    break g;
                }
            },
        };
        // An SPSA iteration's point is drawn as its first game goes out.
        if let Some(spsa) = &mut st.spsa
            && let Err(e) = spsa.open_point(g.0)
        {
            st.retry.push_front(g);
            st.end.get_or_insert(RunEnd::Halted(e.to_string()));
            return None;
        }
        st.running.push(g);
        Some(g)
    }

    /// The options engine `engine` plays pair `pair` with, over its spec's
    /// (an SPSA run's parameters).
    fn game_options(&self, pair: u32, engine: usize) -> Vec<(String, OptionValue)> {
        let st = self.state.lock().unwrap();
        st.spsa.as_ref().map(|s| s.options(pair, engine)).unwrap_or_default()
    }

    /// An SPSA run's progress.
    pub(crate) fn spsa_info(&self) -> Option<SpsaInfo> {
        self.state.lock().unwrap().spsa.as_ref().map(SpsaRun::info)
    }

    /// How the test ended, once it has and its last games are done: None
    /// while it has games running or still to play (held or not).
    pub(crate) fn outcome(&self) -> Option<RunEnd> {
        let st = self.state.lock().unwrap();
        if !st.running.is_empty() {
            return None;
        }
        if let Some(end) = &st.end {
            return Some(end.clone());
        }
        let more = !st.retry.is_empty() || st.max_pairs.is_none_or(|m| self.next_unplayed(&st).0 <= m);
        if !more { Some(RunEnd::MaxPairs) } else { None }
    }

    fn next_unplayed(&self, st: &State) -> (u32, u8) {
        let mut g = st.cursor;
        while st.done.contains(&g) {
            g = if g.1 == 1 { (g.0, 2) } else { (g.0 + 1, 1) };
        }
        g
    }

    /// The games being played: handed out and not yet logged. A slot keeps
    /// a logged game a little longer while it restarts or quits engines
    /// (`busy`), but it's finished.
    pub(crate) fn running(&self) -> Vec<(u32, u8)> {
        let st = self.state.lock().unwrap();
        st.running.iter().filter(|g| !st.done.contains(g)).copied().collect()
    }

    /// How many slots hold one of the test's games, finished or not.
    pub(crate) fn busy(&self) -> usize {
        self.state.lock().unwrap().running.len()
    }

    pub(crate) fn entries(&self) -> Vec<GameEntry> {
        self.state.lock().unwrap().entries.clone()
    }

    pub(crate) fn summary(&self) -> Summary {
        summarize(self.dir.spec.sprt, &self.state.lock().unwrap().entries)
    }

    /// Games at once at most.
    pub(crate) fn concurrency(&self) -> u32 {
        self.concurrency.load(Ordering::SeqCst)
    }

    /// Changes the games at once; games beyond a lowered limit finish.
    pub(crate) fn set_concurrency(&self, n: u32) {
        self.concurrency.store(n.max(1), Ordering::SeqCst);
    }

    /// Whether the SPRT's chance of deciding within the pair limit is
    /// below the test's `undecided_below`, so the test should end
    /// undecided. Never once every pair is played (that's
    /// [`RunEnd::MaxPairs`]).
    pub(crate) fn unlikely_to_decide(&self) -> bool {
        let st = self.state.lock().unwrap();
        self.below_cutoff(&st, &summarize(self.dir.spec.sprt, &st.entries))
    }

    fn below_cutoff(&self, st: &State, summary: &Summary) -> bool {
        let Some(sp) = self.dir.spec.sprt else { return false };
        let (Some(cutoff), Some(max)) = (st.undecided_below, st.max_pairs) else { return false };
        summary.pairs < max && decide_chance(sp, summary, max).is_some_and(|c| c < cutoff)
    }

    /// Ends the test undecided now if it's unlikely to decide in time
    /// (after a change to its limit or cutoff), rather than after its
    /// next game.
    pub(crate) fn end_if_unlikely(&self) {
        let mut st = self.state.lock().unwrap();
        let summary = summarize(self.dir.spec.sprt, &st.entries);
        if st.end.is_none() && self.below_cutoff(&st, &summary) {
            st.end = Some(RunEnd::Undecided);
        }
    }

    pub(crate) fn set_undecided_below(&self, cutoff: Option<f64>) {
        self.state.lock().unwrap().undecided_below = cutoff;
    }

    pub(crate) fn set_max_pairs(&self, max_pairs: Option<u32>) {
        let mut st = self.state.lock().unwrap();
        st.max_pairs = max_pairs;
        // Games handed back beyond a lowered limit aren't wanted any more.
        st.retry.retain(|g| max_pairs.is_none_or(|m| g.0 <= m));
    }

    /// Clears a halt, so a test can be resumed after its engines are fixed.
    /// A decided test stays decided.
    pub(crate) fn clear_halt(&self) -> bool {
        let mut st = self.state.lock().unwrap();
        if matches!(st.end, Some(RunEnd::Decided(_))) {
            return false;
        }
        st.end = None;
        st.failures = [0; 2];
        true
    }

    pub(crate) fn halt(&self, why: String) {
        self.state.lock().unwrap().end.get_or_insert(RunEnd::Halted(why));
    }

    /// Marks a game handed out by [`TestRun::next_game`] as no longer
    /// running (finished, or handed back).
    fn not_running(&self, game: (u32, u8)) {
        let mut st = self.state.lock().unwrap();
        st.running.retain(|g| *g != game);
        st.live.remove(&game);
    }

    /// A running game's moves and clocks so far.
    pub(crate) fn live(&self, game: (u32, u8)) -> Option<LiveGame> {
        let st = self.state.lock().unwrap();
        let live = st.live.get(&game)?;
        let spec = &self.dir.spec;
        let gold = GameEntry::gold_of(game.1);
        let ms = |d: Duration| d.as_millis() as u64;
        let [gold_name, silver_name] = {
            let names = spec.side_names();
            [names[gold].clone(), names[1 - gold].clone()]
        };
        Some(LiveGame {
            pair: game.0,
            game: game.1,
            gold: gold_name,
            silver: silver_name,
            time_control: spec.time_control.clone(),
            setup_minimum: setup_minimum_tag(spec),
            moves: live.moves.clone(),
            used_ms: live.used.iter().copied().map(ms).collect(),
            reserves_ms: live.reserves.map(|r| r.map(ms)),
            turn_elapsed_ms: ms(live.turn_started.elapsed()),
            engine_ids: live.ids.clone(),
            output: live.output.clone().map(Vec::from),
        })
    }

    /// The moves of a running game so far.
    fn live_moves(&self, game: (u32, u8)) -> Vec<String> {
        self.state.lock().unwrap().live.get(&game).map(|l| l.moves.clone()).unwrap_or_default()
    }

    fn start_live(&self, game: (u32, u8), ids: [Option<String>; 2]) {
        let reserves = self.config.time_control.map(|tc| [tc.starting_reserve(); 2]);
        let live = Live {
            moves: Vec::new(),
            used: Vec::new(),
            reserves,
            turn_started: Instant::now(),
            ids,
            output: Default::default(),
        };
        self.state.lock().unwrap().live.insert(game, live);
    }

    /// Keeps an engine's search output in a running game, and passes it on.
    fn live_output(&self, game: (u32, u8), side: Color, line: EngineLine) {
        {
            let mut st = self.state.lock().unwrap();
            let Some(live) = st.live.get_mut(&game) else { return };
            let kept = &mut live.output[side.index()];
            if kept.len() == LINES_KEPT {
                kept.pop_front();
            }
            kept.push_back(line.clone());
        }
        self.send(RunEvent::EngineOutput { pair: game.0, game: game.1, side, line });
    }

    /// Records a move in a running game; returns both reserves after it.
    fn live_move(
        &self,
        game: (u32, u8),
        side: usize,
        notation: &str,
        used: Duration,
        reserve: Option<Duration>,
    ) -> Option<[Duration; 2]> {
        let mut st = self.state.lock().unwrap();
        let live = st.live.get_mut(&game)?;
        live.moves.push(notation.to_string());
        live.used.push(used);
        if let (Some(r), Some(reserves)) = (reserve, live.reserves.as_mut()) {
            reserves[side] = r;
        }
        live.turn_started = Instant::now();
        live.reserves
    }

    /// Puts back a game that couldn't be played, counting a failure for
    /// each engine to blame.
    fn give_back(&self, game: (u32, u8), blame: &[usize], why: &str) {
        let mut st = self.state.lock().unwrap();
        st.retry.push_back(game);
        for &i in blame {
            self.count_failure(&mut st, i, why);
        }
    }

    fn count_failure(&self, st: &mut State, engine: usize, why: &str) {
        st.failures[engine] += 1;
        if st.failures[engine] >= MAX_FAILURES && st.end.is_none() {
            let name = &self.dir.spec.side_names()[engine];
            st.end = Some(RunEnd::Halted(format!(
                "{name} failed {} times in a row; last: {why}",
                st.failures[engine]
            )));
        }
    }

    /// Logs a finished game and updates the failure counts and the end.
    fn finish(&self, entry: GameEntry, failed: Option<usize>) -> Result<Summary, TourneyError> {
        let mut st = self.state.lock().unwrap();
        st.log.append(&entry)?;
        st.done.insert((entry.pair, entry.game));
        // No longer live once logged, before `GameFinished` goes out (the
        // slot only calls `not_running` after restarting engines).
        st.live.remove(&(entry.pair, entry.game));
        for i in 0..2 {
            if failed == Some(i) {
                self.count_failure(&mut st, i, entry.detail.as_deref().unwrap_or("forfeit"));
            } else {
                st.failures[i] = 0;
            }
        }
        let pair = entry.pair;
        let finished = entry.finished;
        st.entries.push(entry);
        let State { spsa, entries, .. } = &mut *st;
        if let Some(spsa) = spsa {
            spsa.game_logged(entries, pair, finished)?;
        }
        let summary = summarize(self.dir.spec.sprt, &st.entries);
        if let Some(d) = summary.decision {
            st.end.get_or_insert(RunEnd::Decided(d));
        } else if st.end.is_none() && self.below_cutoff(&st, &summary) {
            st.end = Some(RunEnd::Undecided);
        }
        Ok(summary)
    }

    fn send(&self, event: RunEvent) {
        (self.on_event)(event);
    }

    /// Tells the slots holding the test's engines to let them go, and its
    /// files (once they drop the test).
    pub(crate) fn retire(&self) {
        self.retired.store(true, Ordering::SeqCst);
        self.retire.notify_waiters();
    }

    /// Resolves once the test is retired.
    async fn retired(&self) {
        loop {
            let notified = self.retire.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.retired.load(Ordering::SeqCst) {
                return;
            }
            notified.await;
        }
    }
}

/// Where slots get their games.
pub(crate) trait Picker: Send + Sync + 'static {
    /// The next game for slot `slot`, which holds engines for `current`.
    /// None ends the slot.
    fn next(
        &self,
        slot: usize,
        current: Option<&Arc<TestRun>>,
    ) -> impl Future<Output = Option<(Arc<TestRun>, (u32, u8))>> + Send;

    /// Called after each game a slot took, played or handed back.
    fn after_game(&self, run: &Arc<TestRun>);

    /// A game's files couldn't be written. The test is halted already;
    /// returning the error ends the slot with it.
    fn failed(&self, run: &Arc<TestRun>, error: TourneyError) -> Option<TourneyError>;

    /// The engines every slot has running, and their budget.
    fn engines(&self) -> &Engines;

    /// Whether slot `slot` should start no more games (it may still be
    /// waiting for room in the budget).
    fn stopping(&self, slot: usize) -> bool;
}

struct OneTest(Arc<TestRun>, Engines);

impl Picker for OneTest {
    async fn next(&self, _slot: usize, _current: Option<&Arc<TestRun>>) -> Option<(Arc<TestRun>, (u32, u8))> {
        self.0.next_game().map(|g| (self.0.clone(), g))
    }

    fn after_game(&self, _run: &Arc<TestRun>) {}

    fn failed(&self, _run: &Arc<TestRun>, error: TourneyError) -> Option<TourneyError> {
        Some(error)
    }

    fn engines(&self) -> &Engines {
        &self.1
    }

    fn stopping(&self, _slot: usize) -> bool {
        self.0.hold.is_stopped()
    }
}

/// Plays a test's remaining games and says why it stopped. Progress goes
/// to `events`. Errors are failures to read or write the test's files.
pub async fn run(
    dir: TestDir,
    options: RunOptions,
    stop: StopHandle,
    events: UnboundedSender<RunEvent>,
) -> Result<RunEnd, TourneyError> {
    let concurrency = options.concurrency.unwrap_or(dir.spec.concurrency).max(1);
    let on_event: EventFn = Box::new(move |e| {
        let _ = events.send(e);
    });
    let test = TestRun::open(dir, concurrency, on_event)?;
    if let Some(end @ RunEnd::Decided(_)) = test.outcome() {
        return Ok(end);
    }
    let test = Arc::new(TestRun { hold: stop.clone(), ..test });
    let budget = options.memory_budget.unwrap_or_else(memory::default_budget);
    let picker = Arc::new(OneTest(test.clone(), Engines::new(budget)));
    let mut slots = JoinSet::new();
    for slot in 0..concurrency as usize {
        slots.spawn(play_slot(picker.clone(), slot));
    }
    let mut error = None;
    while let Some(joined) = slots.join_next().await {
        if let Err(e) = joined.unwrap_or_else(|e| std::panic::resume_unwind(e.into_panic())) {
            error.get_or_insert(e);
        }
    }
    if let Some(e) = error {
        return Err(e);
    }
    Ok(test.outcome().unwrap_or(RunEnd::Stopped))
}

/// The transcript file an engine is writing to, swapped for each game.
type LogSink = Arc<Mutex<Option<BufWriter<File>>>>;

/// An engine a slot holds.
struct Slotted {
    engine: Engine,
    /// Its key in [`Engines`].
    key: u64,
    /// Its resident memory after its first game.
    first: Option<u64>,
}

/// A slot's engines, for the test it's on.
struct Held {
    run: Arc<TestRun>,
    engines: [Option<Slotted>; 2],
    sinks: [LogSink; 2],
}

async fn quit_all(engines: &mut [Option<Slotted>; 2], tracked: &Engines) {
    for slotted in engines.iter_mut().filter_map(Option::take) {
        tracked.remove(slotted.key);
        let _ = slotted.engine.quit(Duration::from_secs(5)).await;
    }
}

pub(crate) async fn play_slot(picker: Arc<impl Picker>, slot: usize) -> Result<(), TourneyError> {
    let tracked = picker.engines();
    let mut held: Option<Held> = None;
    let mut result = Ok(());
    loop {
        if !tracked.room() {
            // Our idle engines count against the budget too.
            if let Some(h) = &mut held {
                quit_all(&mut h.engines, tracked).await;
            }
            while !tracked.room() && !picker.stopping(slot) {
                tokio::time::sleep(memory::POLL).await;
            }
        }
        // Picking is done within one poll, so a timeout (or the test being
        // retired) can't lose a game.
        let waited = {
            let current = held.as_ref().map(|h| &h.run);
            tokio::select! {
                next = tokio::time::timeout(IDLE_KEEP, picker.next(slot, current)) => next.ok(),
                () = async {
                    match current {
                        Some(run) => run.retired().await,
                        None => std::future::pending().await,
                    }
                } => None,
            }
        };
        let next = match waited {
            Some(next) => next,
            None => {
                // Idle or retired: let the engines go, and the test too, so
                // a retired test's files are free.
                if let Some(mut h) = held.take() {
                    quit_all(&mut h.engines, tracked).await;
                }
                picker.next(slot, None).await
            }
        };
        let Some((run, game)) = next else { break };
        if held.as_ref().is_some_and(|h| !Arc::ptr_eq(&h.run, &run))
            && let Some(mut old) = held.take()
        {
            quit_all(&mut old.engines, tracked).await;
        }
        let h = held.get_or_insert_with(|| Held {
            run: run.clone(),
            engines: [None, None],
            sinks: Default::default(),
        });
        let played = play_game(&run, game, &mut h.engines, &h.sinks, tracked).await;
        run.not_running(game);
        if let Err(e) = played {
            run.halt(e.to_string());
            if let Some(e) = picker.failed(&run, e) {
                result = Err(e);
                picker.after_game(&run);
                break;
            }
        }
        picker.after_game(&run);
    }
    if let Some(mut h) = held {
        quit_all(&mut h.engines, tracked).await;
    }
    result
}

/// How a game in play ended.
enum Played {
    Game(Result<MatchOutcome, howdah_aei::AeiError>),
    /// Engine `.0` went over a memory limit, for reason `.1`.
    OverLimit(usize, String),
}

async fn play_game(
    ctx: &TestRun,
    (pair, game): (u32, u8),
    engines: &mut [Option<Slotted>; 2],
    sinks: &[LogSink; 2],
    tracked: &Engines,
) -> Result<(), TourneyError> {
    let mut failed = Vec::new();
    for i in 0..2 {
        if engines[i].is_none() {
            match start_engine(ctx, i, sinks[i].clone()).await {
                Ok(engine) => {
                    let key = tracked.add(engine.pid().unwrap_or(0));
                    engines[i] = Some(Slotted { engine, key, first: None });
                }
                Err(error) => {
                    ctx.send(RunEvent::EngineFailedToStart { engine: i, error: error.clone() });
                    failed.push((i, error));
                }
            }
        }
    }
    if !failed.is_empty() {
        let blame: Vec<usize> = failed.iter().map(|f| f.0).collect();
        ctx.give_back((pair, game), &blame, &format!("didn't start: {}", failed[0].1));
        tokio::time::sleep(RETRY_DELAY).await;
        return Ok(());
    }
    if ctx.dir.spec.engine_logs {
        for (i, sink) in sinks.iter().enumerate() {
            let path = ctx.dir.log_path(pair, game, i);
            let file = File::create(&path).map_err(TourneyError::io(&path))?;
            *sink.lock().unwrap() = Some(BufWriter::new(file));
        }
    }
    // The game's own options (an SPSA run's parameters), over the spec's.
    for i in 0..2 {
        let mut failed = None;
        let engine = &mut engines[i].as_mut().expect("started").engine;
        for (name, value) in ctx.game_options(pair, i) {
            if let Err(e) = engine.set_option(&name, &value).await {
                failed = Some(format!("setting option {name}: {e}"));
                break;
            }
        }
        if let Some(why) = failed {
            restart(ctx, engines, tracked, i, &why).await;
            ctx.give_back((pair, game), &[i], &why);
            return Ok(());
        }
    }

    let gold = GameEntry::gold_of(game);
    let ids = [gold, 1 - gold].map(|i| engines[i].as_ref().and_then(|e| e.engine.id().name.clone()));
    ctx.start_live((pair, game), ids);
    ctx.send(RunEvent::GameStarted { pair, game });
    let keys = [0, 1].map(|i| engines[i].as_ref().expect("started").key);
    let mut used = Vec::new();
    let played = {
        let [a, b] = &mut *engines;
        let (a, b) = (&mut a.as_mut().expect("started").engine, &mut b.as_mut().expect("started").engine);
        let (g, s) = if gold == 0 { (a, b) } else { (b, a) };
        let game_played = play_match(g, s, &ctx.config, |event| match event {
            MatchEvent::MovePlayed { ply, side, notation, used: u, reserve } => {
                used.push(u);
                let reserves = ctx.live_move((pair, game), side.index(), &notation, u, reserve);
                ctx.send(RunEvent::MovePlayed { pair, game, ply, notation, used: u, reserves });
            }
            MatchEvent::Info { side, info } => ctx.live_output((pair, game), side, EngineLine::Info(info)),
            MatchEvent::Log { side, text } => ctx.live_output((pair, game), side, EngineLine::Log(text)),
            _ => {}
        });
        tokio::select! {
            outcome = game_played => Played::Game(outcome),
            (i, why) = watch_memory(ctx, tracked, keys) => Played::OverLimit(i, why),
        }
    };
    for sink in sinks {
        if let Some(mut w) = sink.lock().unwrap().take() {
            let _ = w.flush();
        }
    }

    match played {
        Played::Game(Err(e)) => {
            // It failed before the first move, so it isn't a game. We
            // can't tell which engine to blame: start both afresh.
            let why = format!("failed before the first move: {e}");
            for i in 0..2 {
                restart(ctx, engines, tracked, i, &why).await;
            }
            ctx.give_back((pair, game), &[0, 1], &why);
        }
        Played::Game(Ok(outcome)) => {
            let mut entry = write_game(ctx, pair, game, &outcome, &used)?;
            let loser = 1 - entry.winner;
            let reason = outcome.result.reason;
            let failed = (reason == WinReason::Forfeit).then_some(loser);
            let mut restarts = Vec::new();
            if matches!(reason, WinReason::Forfeit | WinReason::Timeout) {
                restarts.push(Restart { engine: loser, reason: outcome.detail.clone().unwrap_or_default() });
            }
            let rss = keys.map(|k| tracked.read(k));
            for (i, slotted) in engines.iter_mut().enumerate() {
                let (Some(slotted), Some(rss)) = (slotted, rss[i]) else { continue };
                let first = *slotted.first.get_or_insert(rss);
                if !restarts.iter().any(|r| r.engine == i)
                    && let Some(reason) = ctx.limits.restart_reason(first, rss)
                {
                    restarts.push(Restart { engine: i, reason });
                }
            }
            entry.memory_mb = memory_mb(rss);
            entry.restarts = restarts.clone();
            let summary = ctx.finish(entry.clone(), failed)?;
            ctx.send(RunEvent::GameFinished { entry, summary: Box::new(summary) });
            for r in &restarts {
                restart(ctx, engines, tracked, r.engine, &r.reason).await;
            }
            for (i, slot) in engines.iter_mut().enumerate() {
                if ctx.dir.spec.engine(i).restart_each_game
                    && let Some(e) = slot.take()
                {
                    tracked.remove(e.key);
                    let _ = e.engine.quit(Duration::from_secs(5)).await;
                }
            }
        }
        Played::OverLimit(over, why) => {
            // The game is cut short: the engine over its limit forfeits
            // it, and both start afresh (the other may be mid-search).
            let mut record = Game::new();
            for m in ctx.live_moves((pair, game)) {
                if record.play_notation(&m).is_err() {
                    break;
                }
            }
            let side = if over == gold { Color::Gold } else { Color::Silver };
            let result = GameResult { winner: side.opponent(), reason: WinReason::Forfeit };
            record.end_game(result).ok();
            let outcome = MatchOutcome { game: record, result, detail: Some(why.clone()), reserves: None };
            let mut entry = write_game(ctx, pair, game, &outcome, &used)?;
            entry.reason = RESOURCE_LIMIT.into();
            entry.memory_mb = memory_mb(keys.map(|k| tracked.read(k)));
            let other = 1 - over;
            let cut = "its game was cut short by the other engine's memory".to_string();
            entry.restarts =
                vec![Restart { engine: over, reason: why }, Restart { engine: other, reason: cut }];
            let summary = ctx.finish(entry.clone(), Some(over))?;
            ctx.send(RunEvent::GameFinished { entry: entry.clone(), summary: Box::new(summary) });
            if let Some(e) = engines[over].take() {
                tracked.remove(e.key);
                let _ = e.engine.kill().await;
                ctx.send(RunEvent::EngineRestarted {
                    engine: over,
                    reason: entry.restarts[0].reason.clone(),
                });
            }
            restart(ctx, engines, tracked, other, &entry.restarts[1].reason).await;
        }
    }
    Ok(())
}

/// Reads a running game's engines every [`memory::POLL`] and returns the
/// one to stop, and why: one over its limit, or the largest of all the
/// engines when they're over the budget together and it's one of ours.
async fn watch_memory(ctx: &TestRun, tracked: &Engines, keys: [u64; 2]) -> (usize, String) {
    let names = ctx.dir.spec.side_names();
    let limit = ctx.limits.limit;
    loop {
        tokio::time::sleep(memory::POLL).await;
        for (i, key) in keys.iter().enumerate() {
            if let Some(rss) = tracked.read(*key)
                && rss > limit
            {
                let name = &names[i];
                return (i, format!("{name} used {} resident, over its limit of {}", size(rss), size(limit)));
            }
        }
        if let Some((total, largest)) = tracked.over_budget()
            && let Some(i) = keys.iter().position(|k| *k == largest)
        {
            return (
                i,
                format!(
                    "the engines used {} together, over the budget of {}; {} was the largest",
                    size(total),
                    size(tracked.budget()),
                    names[i]
                ),
            );
        }
    }
}

/// Both readings in MB, if both could be read.
fn memory_mb(rss: [Option<u64>; 2]) -> Option<[u64; 2]> {
    Some([rss[0]?, rss[1]?].map(memory::mb))
}

/// Quits engine `i` (it starts again before the next game).
async fn restart(
    ctx: &TestRun,
    engines: &mut [Option<Slotted>; 2],
    tracked: &Engines,
    i: usize,
    reason: &str,
) {
    if let Some(e) = engines[i].take() {
        tracked.remove(e.key);
        let _ = e.engine.quit(Duration::from_secs(2)).await;
        ctx.send(RunEvent::EngineRestarted { engine: i, reason: reason.to_string() });
    }
}

async fn start_engine(ctx: &TestRun, i: usize, sink: LogSink) -> Result<Engine, String> {
    // The kernel's cap, a backstop for the watchdog's limit.
    let config = ctx.dir.engine_config(i).memory_limit(ctx.limits.kernel_cap());
    let transcript = ctx.dir.spec.engine_logs.then(|| {
        Box::new(move |d: Direction, line: &str| {
            if let Some(w) = sink.lock().unwrap().as_mut() {
                let arrow = if d == Direction::ToEngine { '>' } else { '<' };
                let _ = writeln!(w, "{arrow} {line}");
            }
        }) as Box<dyn FnMut(Direction, &str) + Send>
    });
    let mut engine = Engine::start_with_transcript(&config, transcript).await.map_err(|e| e.to_string())?;
    for (name, value) in &ctx.dir.spec.engine(i).options {
        engine.set_option(name, value).await.map_err(|e| format!("setting option {name}: {e}"))?;
    }
    Ok(engine)
}

/// The `SetupMinimum` tag's value for a timed test's games, unless the
/// setup minimum is arimaa.com's minute (which records take as given).
fn setup_minimum_tag(spec: &TestSpec) -> Option<String> {
    let tc = spec.time_control()?;
    (tc.setup_minimum() != DEFAULT_SETUP_MINIMUM).then(|| TimeControl::format_time(tc.setup_minimum()))
}

/// Writes a game's record and makes its log entry.
fn write_game(
    ctx: &TestRun,
    pair: u32,
    game: u8,
    outcome: &MatchOutcome,
    used: &[Duration],
) -> Result<GameEntry, TourneyError> {
    let spec = &ctx.dir.spec;
    let gold = GameEntry::gold_of(game);
    let (tree, _) = GameTree::from_game(&outcome.game);
    let mut record = GameRecord { tags: Vec::new(), tree };
    record.set_tag("Event", spec.name.as_str());
    record.set_tag("Round", format!("{pair}.{game}"));
    let names = spec.side_names();
    record.set_tag("Gold", names[gold].as_str());
    record.set_tag("Silver", names[1 - gold].as_str());
    if let Some(tc) = &spec.time_control {
        record.set_tag("TimeControl", tc.as_str());
    }
    if let Some(minimum) = setup_minimum_tag(spec) {
        record.set_tag(SETUP_MINIMUM_TAG, minimum);
    }
    if let Some(detail) = &outcome.detail {
        record.set_tag("Termination", detail.as_str());
    }
    // A time control with fractions of a second gets move times to the
    // millisecond.
    let precise = spec.time_control().is_some_and(|tc| !tc.is_whole_seconds());
    let line = record.tree.main_line();
    for (node, time) in line.iter().skip(1).zip(used) {
        if let Ok(a) = record.tree.annotation_mut(*node) {
            if precise { a.set_elapsed_ms(*time) } else { a.set_elapsed(*time) }
        }
    }
    let name = TestDir::record_name(pair, game);
    let path = ctx.dir.path.join(&name);
    std::fs::write(&path, record.to_record()).map_err(TourneyError::io(&path))?;

    let mut by_engine = [0.0; 2];
    for (ply, t) in used.iter().enumerate() {
        let side_engine = if ply % 2 == 0 { gold } else { 1 - gold };
        by_engine[side_engine] += t.as_secs_f64();
    }
    let notations: Vec<String> = outcome.game.moves().iter().map(|m| m.notation()).collect();
    let winner_engine = if outcome.result.winner.index() == 0 { gold } else { 1 - gold };
    Ok(GameEntry {
        pair,
        game,
        winner: winner_engine,
        reason: reason_name(outcome.result.reason).into(),
        plies: outcome.game.ply_count() as u32,
        detail: outcome.detail.clone(),
        used: by_engine.map(|s| (s * 1000.0).round() / 1000.0),
        record: name,
        moves_hash: moves_hash(notations.iter().map(String::as_str)),
        finished: SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs()),
        memory_mb: None,
        restarts: Vec::new(),
    })
}

/// The reason a [`GameEntry`]'s `reason` names.
pub(crate) fn reason_from_name(name: &str) -> Option<WinReason> {
    [
        WinReason::Goal,
        WinReason::Elimination,
        WinReason::Immobilization,
        WinReason::Timeout,
        WinReason::Resignation,
        WinReason::IllegalMove,
        WinReason::Score,
        WinReason::Forfeit,
    ]
    .into_iter()
    .find(|r| reason_name(*r) == name)
    // A forfeit for going over a memory limit.
    .or((name == RESOURCE_LIMIT).then_some(WinReason::Forfeit))
}

pub(crate) fn reason_name(reason: WinReason) -> &'static str {
    match reason {
        WinReason::Goal => "goal",
        WinReason::Elimination => "elimination",
        WinReason::Immobilization => "immobilization",
        WinReason::Timeout => "timeout",
        WinReason::Resignation => "resignation",
        WinReason::IllegalMove => "illegalMove",
        WinReason::Score => "score",
        WinReason::Forfeit => "forfeit",
    }
}
