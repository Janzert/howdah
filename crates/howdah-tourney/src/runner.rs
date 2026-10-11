//! Playing a test's games.
//!
//! Each of `concurrency` slots holds a process per engine, reused from game
//! to game, and takes the next game not yet in the results log. A game
//! that ends is written as a record and appended to the log before the
//! next starts, so a run stopped or killed at any point resumes by playing
//! whatever isn't logged.

use std::collections::{HashSet, VecDeque};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use howdah_aei::{Direction, Engine, MatchConfig, MatchEvent, MatchOutcome, play_match};
use howdah_arimaa::{GameRecord, GameTree, WinReason};
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
#[derive(Clone, Debug, PartialEq)]
pub enum RunEnd {
    /// The SPRT decided.
    Decided(Decision),
    /// Every pair up to `max_pairs` was played.
    MaxPairs,
    /// [`StopHandle::stop`] was called; running games were finished.
    Stopped,
    /// Something went wrong that needs a person (an engine failing again
    /// and again).
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
}

struct State {
    entries: Vec<GameEntry>,
    done: HashSet<(u32, u8)>,
    /// Games handed out that couldn't be played, to hand out again first.
    retry: VecDeque<(u32, u8)>,
    /// The next game in order to consider.
    cursor: (u32, u8),
    /// Games each engine has failed in a row.
    failures: [u32; 2],
    end: Option<RunEnd>,
    log: ResultsLog,
}

struct Ctx {
    dir: TestDir,
    config: MatchConfig,
    max_pairs: Option<u32>,
    state: Mutex<State>,
    stop: StopHandle,
    events: UnboundedSender<RunEvent>,
}

/// Plays a test's remaining games and says why it stopped. Progress goes
/// to `events`. Errors are failures to read or write the test's files.
pub async fn run(
    dir: TestDir,
    options: RunOptions,
    stop: StopHandle,
    events: UnboundedSender<RunEvent>,
) -> Result<RunEnd, TourneyError> {
    let entries = dir.results()?;
    if let Some(decision) = summarize(dir.spec.sprt, &entries).decision {
        return Ok(RunEnd::Decided(decision));
    }
    std::fs::create_dir_all(dir.path.join("games")).map_err(TourneyError::io(dir.path.join("games")))?;
    if dir.spec.engine_logs {
        std::fs::create_dir_all(dir.path.join("logs")).map_err(TourneyError::io(dir.path.join("logs")))?;
    }
    let log = ResultsLog::open(&dir.results_path())?;
    let concurrency = options.concurrency.unwrap_or(dir.spec.concurrency).max(1);
    let ctx = Arc::new(Ctx {
        config: MatchConfig { time_control: dir.spec.time_control(), ..MatchConfig::default() },
        max_pairs: dir.spec.max_pairs,
        state: Mutex::new(State {
            done: entries.iter().map(|e| (e.pair, e.game)).collect(),
            entries,
            retry: VecDeque::new(),
            cursor: (1, 1),
            failures: [0; 2],
            end: None,
            log,
        }),
        dir,
        stop,
        events,
    });

    let mut slots = JoinSet::new();
    for _ in 0..concurrency {
        slots.spawn(play_slot(ctx.clone()));
    }
    let mut error = None;
    while let Some(joined) = slots.join_next().await {
        let result = joined.unwrap_or_else(|e| std::panic::resume_unwind(e.into_panic()));
        if let Err(e) = result {
            ctx.state.lock().unwrap().end.get_or_insert(RunEnd::Halted(e.to_string()));
            error.get_or_insert(e);
        }
    }
    if let Some(e) = error {
        return Err(e);
    }
    let end = ctx.state.lock().unwrap().end.clone();
    Ok(end.unwrap_or(if ctx.stop.is_stopped() { RunEnd::Stopped } else { RunEnd::MaxPairs }))
}

impl Ctx {
    fn next_game(&self) -> Option<(u32, u8)> {
        let mut st = self.state.lock().unwrap();
        if st.end.is_some() || self.stop.is_stopped() {
            return None;
        }
        if let Some(g) = st.retry.pop_front() {
            return Some(g);
        }
        loop {
            let g = st.cursor;
            if self.max_pairs.is_some_and(|m| g.0 > m) {
                return None;
            }
            st.cursor = if g.1 == 1 { (g.0, 2) } else { (g.0 + 1, 1) };
            if !st.done.contains(&g) {
                return Some(g);
            }
        }
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
        let _ = self.events.send(event);
    }
}

/// The transcript file an engine is writing to, swapped for each game.
type LogSink = Arc<Mutex<Option<BufWriter<File>>>>;

async fn play_slot(ctx: Arc<Ctx>) -> Result<(), TourneyError> {
    let mut engines: [Option<Engine>; 2] = [None, None];
    let sinks: [LogSink; 2] = Default::default();
    let result = play_games(&ctx, &mut engines, &sinks).await;
    for engine in engines.into_iter().flatten() {
        let _ = engine.quit(Duration::from_secs(5)).await;
    }
    result
}

async fn play_games(
    ctx: &Ctx,
    engines: &mut [Option<Engine>; 2],
    sinks: &[LogSink; 2],
) -> Result<(), TourneyError> {
    while let Some((pair, game)) = ctx.next_game() {
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
            continue;
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
    }
    Ok(())
}

/// Quits engine `i` (it starts again before the next game).
async fn restart(ctx: &Ctx, engines: &mut [Option<Engine>; 2], i: usize, reason: &str) {
    if let Some(e) = engines[i].take() {
        let _ = e.quit(Duration::from_secs(2)).await;
        ctx.send(RunEvent::EngineRestarted { engine: i, reason: reason.to_string() });
    }
}

async fn start_engine(ctx: &Ctx, i: usize, sink: LogSink) -> Result<Engine, String> {
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
    ctx: &Ctx,
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
