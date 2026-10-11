//! Playing a test's games.
//!
//! A [`TestRun`] is one test's games in play: which are done, which are
//! running, its engines' failures and its end. Slots play the games: each
//! holds a process per engine, reused from game to game while it stays on
//! the same test, and asks a picker for its next game: for [`run`] the one
//! test, for the daemon its queue. A game that ends is written as a record
//! and appended to the log before the slot moves on, so a run stopped or
//! killed at any point resumes by playing whatever isn't logged.

use std::collections::{HashSet, VecDeque};
use std::fs::File;
use std::future::Future;
use std::io::{BufWriter, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use howdah_aei::{Direction, Engine, MatchConfig, MatchEvent, MatchOutcome, play_match};
use howdah_arimaa::{GameRecord, GameTree, WinReason};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::UnboundedSender;
use tokio::task::JoinSet;

use crate::error::TourneyError;
use crate::results::{GameEntry, ResultsLog, moves_hash};
use crate::stats::{Decision, Summary, summarize};
use crate::testdir::TestDir;

/// An engine failing this many games in a row (crashing, or not starting)
/// halts the test, rather than piling up losses.
const MAX_FAILURES: u32 = 3;

/// How long to wait before trying again after an engine didn't start.
const RETRY_DELAY: Duration = Duration::from_secs(1);

#[derive(Clone, Debug, Default)]
pub struct RunOptions {
    /// Games at once, instead of the spec's.
    pub concurrency: Option<u32>,
}

#[derive(Clone, Debug)]
pub enum RunEvent {
    GameStarted {
        pair: u32,
        game: u8,
    },
    GameFinished {
        entry: GameEntry,
        summary: Box<Summary>,
    },
    /// An engine was restarted after failing, or timing out (it may still
    /// be searching).
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
    /// Games each engine has failed in a row.
    failures: [u32; 2],
    end: Option<RunEnd>,
    log: ResultsLog,
}

/// One test's games in play.
pub(crate) struct TestRun {
    pub(crate) dir: TestDir,
    config: MatchConfig,
    /// Games at once at most.
    concurrency: u32,
    state: Mutex<State>,
    /// Set while the test should start no more games.
    pub(crate) hold: StopHandle,
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
        Ok(TestRun {
            config: MatchConfig { time_control: dir.spec.time_control(), ..MatchConfig::default() },
            concurrency: concurrency.max(1),
            state: Mutex::new(State {
                done: entries.iter().map(|e| (e.pair, e.game)).collect(),
                entries,
                running: Vec::new(),
                retry: VecDeque::new(),
                cursor: (1, 1),
                max_pairs: dir.spec.max_pairs,
                failures: [0; 2],
                end,
                log,
            }),
            dir,
            hold: StopHandle::default(),
            on_event,
        })
    }

    /// Takes the next game to play, if the test has one to start now.
    pub(crate) fn next_game(&self) -> Option<(u32, u8)> {
        let mut st = self.state.lock().unwrap();
        if st.end.is_some() || self.hold.is_stopped() || st.running.len() as u32 >= self.concurrency {
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
        st.running.push(g);
        Some(g)
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

    pub(crate) fn running(&self) -> Vec<(u32, u8)> {
        self.state.lock().unwrap().running.clone()
    }

    pub(crate) fn entries(&self) -> Vec<GameEntry> {
        self.state.lock().unwrap().entries.clone()
    }

    pub(crate) fn summary(&self) -> Summary {
        summarize(self.dir.spec.sprt, &self.state.lock().unwrap().entries)
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
        self.state.lock().unwrap().running.retain(|g| *g != game);
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
            let name = &self.dir.spec.engines[engine].name;
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
        for i in 0..2 {
            if failed == Some(i) {
                self.count_failure(&mut st, i, entry.detail.as_deref().unwrap_or("forfeit"));
            } else {
                st.failures[i] = 0;
            }
        }
        st.entries.push(entry);
        let summary = summarize(self.dir.spec.sprt, &st.entries);
        if let Some(d) = summary.decision {
            st.end.get_or_insert(RunEnd::Decided(d));
        }
        Ok(summary)
    }

    fn send(&self, event: RunEvent) {
        (self.on_event)(event);
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
}

struct OneTest(Arc<TestRun>);

impl Picker for OneTest {
    async fn next(&self, _slot: usize, _current: Option<&Arc<TestRun>>) -> Option<(Arc<TestRun>, (u32, u8))> {
        self.0.next_game().map(|g| (self.0.clone(), g))
    }

    fn after_game(&self, _run: &Arc<TestRun>) {}

    fn failed(&self, _run: &Arc<TestRun>, error: TourneyError) -> Option<TourneyError> {
        Some(error)
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
    let picker = Arc::new(OneTest(test.clone()));
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

/// A slot's engines, for the test it's on.
struct Held {
    run: Arc<TestRun>,
    engines: [Option<Engine>; 2],
    sinks: [LogSink; 2],
}

async fn quit_all(engines: [Option<Engine>; 2]) {
    for engine in engines.into_iter().flatten() {
        let _ = engine.quit(Duration::from_secs(5)).await;
    }
}

pub(crate) async fn play_slot(picker: Arc<impl Picker>, slot: usize) -> Result<(), TourneyError> {
    let mut held: Option<Held> = None;
    let mut result = Ok(());
    while let Some((run, game)) = picker.next(slot, held.as_ref().map(|h| &h.run)).await {
        if held.as_ref().is_some_and(|h| !Arc::ptr_eq(&h.run, &run))
            && let Some(old) = held.take()
        {
            quit_all(old.engines).await;
        }
        let h = held.get_or_insert_with(|| Held {
            run: run.clone(),
            engines: [None, None],
            sinks: Default::default(),
        });
        let played = play_game(&run, game, &mut h.engines, &h.sinks).await;
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
    if let Some(h) = held {
        quit_all(h.engines).await;
    }
    result
}

async fn play_game(
    ctx: &TestRun,
    (pair, game): (u32, u8),
    engines: &mut [Option<Engine>; 2],
    sinks: &[LogSink; 2],
) -> Result<(), TourneyError> {
    let mut failed = Vec::new();
    for i in 0..2 {
        if engines[i].is_none() {
            match start_engine(ctx, i, sinks[i].clone()).await {
                Ok(e) => engines[i] = Some(e),
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

    ctx.send(RunEvent::GameStarted { pair, game });
    let gold = GameEntry::gold_of(game);
    let [a, b] = engines;
    let (a, b) = (a.as_mut().expect("started"), b.as_mut().expect("started"));
    let (g, s) = if gold == 0 { (a, b) } else { (b, a) };
    let mut used = Vec::new();
    let played = play_match(g, s, &ctx.config, |event| {
        if let MatchEvent::MovePlayed { used: u, .. } = event {
            used.push(u);
        }
    })
    .await;
    for sink in sinks {
        if let Some(mut w) = sink.lock().unwrap().take() {
            let _ = w.flush();
        }
    }

    match played {
        Err(e) => {
            // It failed before the first move, so it isn't a game. We
            // can't tell which engine to blame: start both afresh.
            let why = format!("failed before the first move: {e}");
            for i in 0..2 {
                restart(ctx, engines, i, &why).await;
            }
            ctx.give_back((pair, game), &[0, 1], &why);
        }
        Ok(outcome) => {
            let entry = write_game(ctx, pair, game, &outcome, &used)?;
            let loser = 1 - entry.winner;
            let reason = outcome.result.reason;
            let failed = (reason == WinReason::Forfeit).then_some(loser);
            let detail = outcome.detail.clone().unwrap_or_default();
            let summary = ctx.finish(entry.clone(), failed)?;
            ctx.send(RunEvent::GameFinished { entry, summary: Box::new(summary) });
            if matches!(reason, WinReason::Forfeit | WinReason::Timeout) {
                restart(ctx, engines, loser, &detail).await;
            }
            for (slot, spec) in engines.iter_mut().zip(&ctx.dir.spec.engines) {
                if spec.restart_each_game
                    && let Some(e) = slot.take()
                {
                    let _ = e.quit(Duration::from_secs(5)).await;
                }
            }
        }
    }
    Ok(())
}

/// Quits engine `i` (it starts again before the next game).
async fn restart(ctx: &TestRun, engines: &mut [Option<Engine>; 2], i: usize, reason: &str) {
    if let Some(e) = engines[i].take() {
        let _ = e.quit(Duration::from_secs(2)).await;
        ctx.send(RunEvent::EngineRestarted { engine: i, reason: reason.to_string() });
    }
}

async fn start_engine(ctx: &TestRun, i: usize, sink: LogSink) -> Result<Engine, String> {
    let config = ctx.dir.engine_config(i);
    let transcript = ctx.dir.spec.engine_logs.then(|| {
        Box::new(move |d: Direction, line: &str| {
            if let Some(w) = sink.lock().unwrap().as_mut() {
                let arrow = if d == Direction::ToEngine { '>' } else { '<' };
                let _ = writeln!(w, "{arrow} {line}");
            }
        }) as Box<dyn FnMut(Direction, &str) + Send>
    });
    let mut engine = Engine::start_with_transcript(&config, transcript).await.map_err(|e| e.to_string())?;
    for (name, value) in &ctx.dir.spec.engines[i].options {
        engine.set_option(name, value).await.map_err(|e| format!("setting option {name}: {e}"))?;
    }
    Ok(engine)
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
    record.set_tag("Gold", spec.engines[gold].name.as_str());
    record.set_tag("Silver", spec.engines[1 - gold].name.as_str());
    if let Some(tc) = &spec.time_control {
        record.set_tag("TimeControl", tc.as_str());
    }
    if let Some(detail) = &outcome.detail {
        record.set_tag("Termination", detail.as_str());
    }
    let line = record.tree.main_line();
    for (node, time) in line.iter().skip(1).zip(used) {
        if let Ok(a) = record.tree.annotation_mut(*node) {
            a.set_elapsed(*time);
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
    })
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
