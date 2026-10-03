//! Background game controller: runs engine players, clocks and analysis.
//!
//! One coordinator task owns the engine actors (one task per engine
//! process). It watches the session for an engine to move, asks that
//! engine's actor to think, sends `stop` shortly before the deadline, flags
//! timeouts, and applies replies through the session. The session stays the
//! only place game state changes, and every change goes out as
//! `game://changed`. Engine search output goes out as `engine://output`.
//!
//! Analysis runs next to the match engines, with its own actor: while it's
//! on, the engine searches the shown node until the board moves elsewhere,
//! and what it finds goes out as `analysis://update`, at most every 100 ms.
//!
//! Commands call [`Controller::poke`] after changing the session, so the
//! coordinator re-checks whose turn it is and what to analyse.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use arimaa_aei::{AeiError, Engine, EngineMessage, Info, Profile, Score, SearchLog};
use arimaa_core::{Color, Game, TimeControl, notation};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::backend::{Events, emit, emit_session};
use crate::dto::{
    AnalysisLine, AnalysisState, AnalysisView, EngineOutput, EngineOutputKind, EngineSpec, Eval, PvTurn,
};
use crate::engines::{EngineRegistry, engine_config};
use crate::session::{AnalysisEngine, AnalysisTarget, Session, depth_key, steps_view};

/// Event carrying an [`EngineOutput`].
pub const ENGINE_OUTPUT: &str = "engine://output";

/// Event carrying an [`AnalysisView`].
pub const ANALYSIS_UPDATE: &str = "analysis://update";

/// Longest `stop` is sent before a deadline.
const STOP_MARGIN: Duration = Duration::from_secs(1);

/// How long an engine has to answer `stop` before a new search.
const STOP_TIMEOUT: Duration = Duration::from_secs(2);

/// Analysis updates go out at most this often.
const ANALYSIS_INTERVAL: Duration = Duration::from_millis(100);

pub type SharedSession = Arc<Mutex<Session>>;
pub type SharedRegistry = Arc<Mutex<EngineRegistry>>;

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

#[derive(Clone)]
pub struct Controller {
    tx: UnboundedSender<ControlMsg>,
}

impl Controller {
    /// The session changed; check whether an engine should move, and what
    /// to analyse.
    pub fn poke(&self) {
        let _ = self.tx.send(ControlMsg::Poke);
    }

    /// Ask the thinking engine to move now (sends `stop`).
    pub fn move_now(&self) {
        let _ = self.tx.send(ControlMsg::MoveNow);
    }
}

/// Which actor a message comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Origin {
    Player {
        generation: u64,
        side: Color,
    },
    /// Each analysis engine started gets a new run number.
    Analysis {
        run: u64,
    },
}

/// What an engine process is for. Analysis gets the profile's analysis
/// options (searching until `stop`, reporting progress).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Purpose {
    Play,
    Analysis,
}

enum ControlMsg {
    Poke,
    MoveNow,
    FromActor { origin: Origin, request: u64, event: ActorEvent },
}

enum ActorEvent {
    Line(EngineMessage),
    Started { name: String, profile: Profile },
    BestMove(String),
    Failed(String),
}

enum ActorCmd {
    Think { request: u64, moves: Vec<String>, tc: Option<TimeControl>, reserves: Option<[Duration; 2]> },
    Stop,
}

struct ActorHandle {
    engine_id: String,
    cmd: UnboundedSender<ActorCmd>,
    profile: Profile,
}

struct Pending {
    request: u64,
    side: Color,
    ply: usize,
    stop_at: Option<Instant>,
    stop_sent: bool,
}

/// The analysis engine and what it's searching.
struct Analysis {
    run: u64,
    engine_id: String,
    name: String,
    cmd: UnboundedSender<ActorCmd>,
    profile: Profile,
    started: bool,
    /// The node being searched; `None` when there's nothing to search.
    target: Option<AnalysisTarget>,
    /// The request searching `target`; output from older ones is dropped.
    request: u64,
    state: AnalysisState,
    /// What this search has found so far.
    line: Option<AnalysisLine>,
    /// Engine output not sent yet.
    log: Vec<String>,
    /// A PV was cut short at an illegal move in this search (logged once).
    pv_cut: bool,
    /// Something changed since the last update.
    dirty: bool,
    sent_at: Option<Instant>,
}

pub fn spawn(events: Events, session: SharedSession, registry: SharedRegistry) -> Controller {
    let (controller, run) = new(events, session, registry);
    tauri::async_runtime::spawn(run);
    controller
}

/// The controller and the coordinator task to run on a Tokio runtime.
fn new(
    events: Events,
    session: SharedSession,
    registry: SharedRegistry,
) -> (Controller, impl Future<Output = ()> + Send + 'static) {
    let (tx, rx) = unbounded_channel();
    let coordinator = Coordinator {
        events,
        session,
        registry,
        tx: tx.clone(),
        generation: None,
        actors: [None, None],
        failed: [false, false],
        pending: None,
        next_request: 1,
        analysis: None,
        next_run: 1,
    };
    (Controller { tx }, coordinator.run(rx))
}

struct Coordinator {
    events: Events,
    session: SharedSession,
    registry: SharedRegistry,
    tx: UnboundedSender<ControlMsg>,
    generation: Option<u64>,
    actors: [Option<ActorHandle>; 2],
    /// Sides whose engine failed in this generation; not restarted.
    failed: [bool; 2],
    pending: Option<Pending>,
    next_request: u64,
    analysis: Option<Analysis>,
    next_run: u64,
}

impl Coordinator {
    async fn run(mut self, mut rx: UnboundedReceiver<ControlMsg>) {
        loop {
            self.sync();
            let timer = self.next_timer();
            tokio::select! {
                msg = rx.recv() => match msg {
                    Some(msg) => self.handle(msg),
                    None => return,
                },
                _ = sleep_until(timer), if timer.is_some() => self.on_timer(),
            }
        }
    }

    fn emit(&self, session: &Session, animation: Vec<crate::dto::AnimStep>) {
        emit_session(&self.events, session, animation, None);
    }

    fn output(&self, side: Color, kind: EngineOutputKind, text: String) {
        let out = EngineOutput { side, kind, text, depth: None, score: None, pv: None };
        emit(&self.events, ENGINE_OUTPUT, out);
    }

    fn sync(&mut self) {
        self.sync_players();
        self.sync_analysis();
    }

    /// Brings the match actors and the pending request in line with the session.
    fn sync_players(&mut self) {
        let (generation, engines, turn) = {
            let s = lock(&self.session);
            (s.generation(), s.engine_players(), s.engine_turn())
        };
        if self.generation != Some(generation) {
            // New game or match: dropping the actors' senders quits them.
            self.actors = [None, None];
            self.failed = [false, false];
            self.pending = None;
            self.generation = Some(generation);
        }
        for side in Color::ALL {
            let wanted = &engines[side.index()];
            let current = self.actors[side.index()].as_ref().map(|a| &a.engine_id);
            if wanted.as_ref() != current && !self.failed[side.index()] {
                self.actors[side.index()] =
                    wanted.as_ref().and_then(|id| self.start_actor(generation, side, id));
            }
        }
        if let Some(turn) = turn
            && self.pending.is_none()
            && let Some(actor) = &self.actors[turn.side.index()]
        {
            let request = self.next_request;
            self.next_request += 1;
            let stop_at = turn.deadline.map(|d| {
                let allowance = d.saturating_duration_since(Instant::now());
                d - STOP_MARGIN.min(allowance / 4)
            });
            let cmd = ActorCmd::Think {
                request,
                moves: turn.moves,
                tc: turn.time_control,
                reserves: turn.reserves,
            };
            if actor.cmd.send(cmd).is_ok() {
                self.pending =
                    Some(Pending { request, side: turn.side, ply: turn.ply, stop_at, stop_sent: false });
                let mut s = lock(&self.session);
                s.set_thinking(Some(turn.side));
                self.emit(&s, Vec::new());
            }
        }
    }

    fn start_actor(&mut self, generation: u64, side: Color, engine_id: &str) -> Option<ActorHandle> {
        let spec = lock(&self.registry).get(engine_id);
        let Some(spec) = spec else {
            self.failed[side.index()] = true;
            let mut s = lock(&self.session);
            s.engine_failed(generation, side, format!("engine {engine_id:?} isn't configured"));
            self.emit(&s, Vec::new());
            return None;
        };
        let (cmd_tx, cmd_rx) = unbounded_channel();
        let origin = Origin::Player { generation, side };
        tokio::spawn(run_actor(spec, Purpose::Play, origin, cmd_rx, self.tx.clone()));
        Some(ActorHandle { engine_id: engine_id.to_string(), cmd: cmd_tx, profile: Profile::Generic })
    }

    /// Starts, stops or switches the analysis engine as the session asks,
    /// and points it at the session's analysis target. Only the latest
    /// target is ever sent; the actor drops searches overtaken by a newer
    /// one before starting them.
    fn sync_analysis(&mut self) {
        let (wanted, target) = {
            let s = lock(&self.session);
            (s.analysis_engine().cloned(), s.analysis_target())
        };
        let mut changed = false;
        if self.analysis.as_ref().map(|a| &a.engine_id) != wanted.as_ref().map(|w| &w.id) {
            // Dropping the old actor's sender quits its engine.
            let was_on = self.analysis.take().is_some();
            match &wanted {
                Some(w) => {
                    self.analysis = self.start_analysis(w);
                    changed = true;
                }
                None if was_on => self.send_analysis_view(AnalysisView::off()),
                None => {}
            }
        }
        let Some(a) = &mut self.analysis else { return };
        let same = match (&a.target, &target) {
            (Some(old), Some(new)) => old.same(new),
            (old, new) => old.is_none() && new.is_none(),
        };
        if !same {
            match target {
                Some(t) => {
                    a.request = self.next_request;
                    self.next_request += 1;
                    let cmd = ActorCmd::Think {
                        request: a.request,
                        moves: t.moves.clone(),
                        tc: None,
                        reserves: None,
                    };
                    // If the actor is gone, its failure report is on the way.
                    let _ = a.cmd.send(cmd);
                    a.state = if a.started { AnalysisState::Searching } else { AnalysisState::Starting };
                    a.target = Some(t);
                }
                None => {
                    let _ = a.cmd.send(ActorCmd::Stop);
                    a.target = None;
                    if a.started {
                        a.state = AnalysisState::Idle;
                    }
                }
            }
            a.line = None;
            a.pv_cut = false;
            changed = true;
        }
        if changed {
            self.flush_analysis(true);
        }
    }

    fn start_analysis(&mut self, engine: &AnalysisEngine) -> Option<Analysis> {
        let spec = lock(&self.registry).get(&engine.id);
        let Some(spec) = spec else {
            self.analysis_failed(engine, format!("engine {:?} isn't configured", engine.name));
            return None;
        };
        let run = self.next_run;
        self.next_run += 1;
        let (cmd_tx, cmd_rx) = unbounded_channel();
        let origin = Origin::Analysis { run };
        tokio::spawn(run_actor(spec, Purpose::Analysis, origin, cmd_rx, self.tx.clone()));
        Some(Analysis {
            run,
            engine_id: engine.id.clone(),
            name: engine.name.clone(),
            cmd: cmd_tx,
            profile: Profile::Generic,
            started: false,
            target: None,
            request: 0,
            state: AnalysisState::Starting,
            line: None,
            log: Vec::new(),
            pv_cut: false,
            dirty: true,
            sent_at: None,
        })
    }

    /// The analysis engine failed: analysis is turned off (if it's still
    /// this engine's) and the panel says why. Nothing in the game changes.
    fn analysis_failed(&mut self, engine: &AnalysisEngine, detail: String) {
        self.analysis = None;
        {
            let mut s = lock(&self.session);
            if s.analysis_engine().is_some_and(|a| a.id == engine.id) {
                s.set_analysis(None);
                self.emit(&s, Vec::new());
            }
        }
        let view = AnalysisView {
            state: AnalysisState::Failed,
            engine: Some(engine.name.clone()),
            detail: Some(detail),
            ..AnalysisView::off()
        };
        self.send_analysis_view(view);
    }

    fn send_analysis_view(&self, view: AnalysisView) {
        emit(&self.events, ANALYSIS_UPDATE, view);
    }

    /// Sends the analysis update if one is due (or at once with `force`),
    /// and keeps the search's line with its node in the session.
    fn flush_analysis(&mut self, force: bool) {
        let Some(a) = &mut self.analysis else { return };
        let now = Instant::now();
        let due = a.sent_at.is_none_or(|t| now >= t + ANALYSIS_INTERVAL);
        if !(force || (a.dirty && due)) {
            return;
        }
        let (line, stored) = match &a.target {
            Some(t) => {
                let mut s = lock(&self.session);
                let kept = s.stored_analysis(t.generation, t.node).cloned();
                if let Some(l) = &a.line {
                    s.store_analysis(t.generation, l);
                }
                match (&a.line, kept) {
                    (Some(l), Some(k)) if depth_key(k.depth.as_deref()) > depth_key(l.depth.as_deref()) => {
                        (Some(k), true)
                    }
                    (Some(l), _) => (Some(l.clone()), false),
                    (None, k) => (k.clone(), k.is_some()),
                }
            }
            None => (None, false),
        };
        let view = AnalysisView {
            state: a.state,
            engine: Some(a.name.clone()),
            node: a.target.as_ref().map(|t| t.node),
            label: a.target.as_ref().map(|t| t.label.clone()),
            line,
            stored,
            log: std::mem::take(&mut a.log),
            detail: None,
        };
        a.dirty = false;
        a.sent_at = Some(now);
        emit(&self.events, ANALYSIS_UPDATE, view);
    }

    fn next_timer(&self) -> Option<Instant> {
        let stop = self.pending.as_ref().filter(|p| !p.stop_sent).and_then(|p| p.stop_at);
        let deadline = lock(&self.session).turn_deadline();
        let analysis = self
            .analysis
            .as_ref()
            .filter(|a| a.dirty)
            .map(|a| a.sent_at.map_or_else(Instant::now, |t| t + ANALYSIS_INTERVAL));
        [stop, deadline, analysis].into_iter().flatten().min()
    }

    fn on_timer(&mut self) {
        self.flush_analysis(false);
        let now = Instant::now();
        if let Some(p) = &mut self.pending
            && !p.stop_sent
            && p.stop_at.is_some_and(|t| now >= t)
        {
            p.stop_sent = true;
            if let Some(actor) = &self.actors[p.side.index()] {
                let _ = actor.cmd.send(ActorCmd::Stop);
            }
        }
        let mut s = lock(&self.session);
        if s.check_timeout(now) {
            self.emit(&s, Vec::new());
            drop(s);
            // The engine (if any) may still be thinking; its reply will be stale.
            if let Some(p) = self.pending.take()
                && let Some(actor) = &self.actors[p.side.index()]
            {
                let _ = actor.cmd.send(ActorCmd::Stop);
            }
        }
    }

    fn handle(&mut self, msg: ControlMsg) {
        match msg {
            ControlMsg::Poke => {}
            ControlMsg::MoveNow => {
                if let Some(p) = &mut self.pending
                    && let Some(actor) = &self.actors[p.side.index()]
                {
                    p.stop_sent = true;
                    let _ = actor.cmd.send(ActorCmd::Stop);
                }
            }
            ControlMsg::FromActor { origin: Origin::Player { generation, side }, request, event } => {
                if self.generation != Some(generation) {
                    return;
                }
                self.on_player_event(generation, side, request, event);
            }
            ControlMsg::FromActor { origin: Origin::Analysis { run }, request, event } => {
                self.on_analysis_event(run, request, event);
            }
        }
    }

    fn on_player_event(&mut self, generation: u64, side: Color, request: u64, event: ActorEvent) {
        match event {
            ActorEvent::Line(msg) => {
                let profile = self.actors[side.index()].as_ref().map_or(Profile::Generic, |a| a.profile);
                if let Some(out) = engine_output(side, profile, msg) {
                    emit(&self.events, ENGINE_OUTPUT, out);
                }
            }
            ActorEvent::Started { name, profile } => {
                if let Some(actor) = &mut self.actors[side.index()] {
                    actor.profile = profile;
                }
                self.output(side, EngineOutputKind::Status, format!("{name} ready"));
            }
            ActorEvent::BestMove(text) => {
                let Some(p) = self.pending.take_if(|p| p.request == request) else { return };
                self.output(side, EngineOutputKind::Status, format!("bestmove {text}"));
                let mut s = lock(&self.session);
                if let Ok(anim) = s.apply_engine_move(generation, p.side, p.ply, &text) {
                    emit_session(&self.events, &s, anim, s.last_move_time());
                }
            }
            ActorEvent::Failed(detail) => {
                self.actors[side.index()] = None;
                self.failed[side.index()] = true;
                if self.pending.as_ref().is_some_and(|p| p.side == side) {
                    self.pending = None;
                }
                self.output(side, EngineOutputKind::Status, detail.clone());
                let mut s = lock(&self.session);
                s.engine_failed(generation, side, detail);
                self.emit(&s, Vec::new());
            }
        }
    }

    fn on_analysis_event(&mut self, run: u64, request: u64, event: ActorEvent) {
        let Some(a) = self.analysis.as_mut().filter(|a| a.run == run) else { return };
        match event {
            ActorEvent::Started { name, profile } => {
                a.profile = profile;
                a.started = true;
                if a.state == AnalysisState::Starting {
                    a.state = if a.target.is_some() { AnalysisState::Searching } else { AnalysisState::Idle };
                }
                a.log.push(format!("{name} ready"));
                a.dirty = true;
            }
            ActorEvent::Line(msg) => {
                let Some(search) = read_search(a.profile, msg) else { return };
                if request == 0 {
                    // Before any search: what it said about its options.
                    a.log.push(search.text);
                    a.dirty = true;
                } else if request == a.request && a.target.is_some() {
                    a.read(search);
                }
            }
            ActorEvent::BestMove(text) => {
                if request != a.request || a.target.is_none() {
                    return;
                }
                a.state = AnalysisState::Finished;
                a.read_best_move(text);
                self.flush_analysis(true);
            }
            ActorEvent::Failed(detail) => {
                let engine = AnalysisEngine { id: a.engine_id.clone(), name: a.name.clone() };
                self.analysis_failed(&engine, detail);
            }
        }
    }
}

impl Analysis {
    /// Takes in the engine's answer. Without a PV (Sharp sends none for a
    /// setup), the move it chose is the line.
    fn read_best_move(&mut self, text: String) {
        let no_pv = self.line.as_ref().is_none_or(|l| l.pv.is_empty());
        let playable = !text.trim().is_empty() && !text.trim().eq_ignore_ascii_case("resign");
        let search = Search {
            kind: EngineOutputKind::Info,
            text: format!("bestmove {text}"),
            depth: None,
            score: None,
            pv: (no_pv && playable).then(|| vec![text]),
            nodes: None,
            time: None,
        };
        self.read(search);
    }

    /// Takes in one line of the current search's output.
    fn read(&mut self, search: Search) {
        let Some(t) = &self.target else { return };
        let progress = search.progress();
        if !search.text.is_empty() {
            self.log.push(search.text);
        }
        if !progress {
            return;
        }
        let mover = t.game.current_position().side_to_move();
        let line = self.line.get_or_insert_with(|| AnalysisLine {
            node: t.node,
            depth: None,
            eval: None,
            pv: Vec::new(),
            nodes: None,
            time_ms: None,
        });
        if let Some(d) = search.depth {
            line.depth = Some(d);
        }
        if let Some(score) = search.score {
            line.eval = Some(eval_for_gold(score, mover));
        }
        if let Some(n) = search.nodes {
            line.nodes = Some(n);
        }
        if let Some(t) = search.time {
            line.time_ms = Some((t * 1000.0).round() as u64);
        }
        if let Some(pv) = search.pv {
            let (turns, cut) = validate_pv(&t.game, &pv);
            line.pv = turns;
            if let Some(bad) = cut
                && !self.pv_cut
            {
                self.pv_cut = true;
                self.log.push(format!("PV cut short at an illegal turn: {bad}"));
            }
        }
        self.dirty = true;
    }
}

impl AnalysisView {
    fn off() -> AnalysisView {
        AnalysisView {
            state: AnalysisState::Off,
            engine: None,
            node: None,
            label: None,
            line: None,
            stored: false,
            log: Vec::new(),
            detail: None,
        }
    }
}

/// A score from the mover's side as an eval from gold's.
fn eval_for_gold(score: Score, mover: Color) -> Eval {
    let score = if mover == Color::Silver { score.flipped() } else { score };
    match score {
        Score::CentiRabbits(value) => Eval::CentiRabbits { value },
        Score::Win => Eval::Decided { winner: Color::Gold },
        Score::Loss => Eval::Decided { winner: Color::Silver },
    }
}

/// Checks a PV turn by turn from the end of `game`, keeping the legal turns
/// up to the first that isn't (returned too) or that ends the game.
fn validate_pv(game: &Game, turns: &[String]) -> (Vec<PvTurn>, Option<String>) {
    let mut game = game.clone();
    let mut out = Vec::new();
    for turn in turns {
        let ply = game.ply_count();
        let mover = game.current_position().side_to_move();
        match game.play_notation(turn) {
            Ok(result) => {
                let mv = &game.moves()[ply];
                out.push(PvTurn {
                    label: notation::move_label(ply),
                    notation: mv.notation(),
                    steps: steps_view(mover, mv),
                });
                if result.is_some() {
                    break;
                }
            }
            Err(_) => return (out, Some(turn.clone())),
        }
    }
    (out, None)
}

async fn sleep_until(t: Option<Instant>) {
    if let Some(t) = t {
        tokio::time::sleep_until(tokio::time::Instant::from_std(t)).await;
    }
}

/// One engine process. Keeps the engine's view of the game in sync with the
/// move list it's given, thinks on request, and reports everything back.
///
/// A `Think` while the engine is searching first stops the search and waits
/// for its `bestmove`, so moves never reach an engine mid-search (analysis
/// does this; a match never does). Of several `Think`s waiting, only the
/// newest is searched.
async fn run_actor(
    spec: EngineSpec,
    purpose: Purpose,
    origin: Origin,
    mut cmds: UnboundedReceiver<ActorCmd>,
    out: UnboundedSender<ControlMsg>,
) {
    let report = |request: u64, event: ActorEvent| {
        let _ = out.send(ControlMsg::FromActor { origin, request, event });
    };
    let mut engine = match Engine::start(&engine_config(&spec)).await {
        Ok(e) => e,
        Err(e) => {
            report(0, ActorEvent::Failed(format!("{} failed to start: {e}", spec.name)));
            return;
        }
    };
    let profile = Profile::detect(engine.id());
    // What the engine says about its options (errors, mostly) is passed on.
    let options = send_options(&mut engine, &spec, profile, purpose).await;
    match options.and(engine.is_ready(Duration::from_secs(10)).await) {
        Ok(said) => said.into_iter().for_each(|m| report(0, ActorEvent::Line(m))),
        Err(e) => {
            report(0, ActorEvent::Failed(format!("{} failed: {e}", spec.name)));
            return;
        }
    }
    report(0, ActorEvent::Started { name: engine.name().to_string(), profile });
    // Moves the engine has been told, or None before its first `newgame`.
    let mut told: Option<Vec<String>> = None;
    let mut thinking: Option<u64> = None;
    let mut stop_sent = false;
    loop {
        let first = tokio::select! {
            cmd = cmds.recv() => match cmd {
                None => {
                    engine.quit(Duration::from_secs(2)).await.ok();
                    return;
                }
                Some(cmd) => cmd,
            },
            msg = engine.recv(), if thinking.is_some() => {
                let request = thinking.expect("guarded");
                match msg {
                    Ok(EngineMessage::BestMove(text)) => {
                        thinking = None;
                        report(request, ActorEvent::BestMove(text));
                    }
                    Ok(other) => report(request, ActorEvent::Line(other)),
                    Err(e) => {
                        report(request, ActorEvent::Failed(format!("{} failed: {e}", spec.name)));
                        return;
                    }
                }
                continue;
            }
        };
        let mut queue = vec![first];
        while let Ok(cmd) = cmds.try_recv() {
            queue.push(cmd);
        }
        let newest = queue.iter().rposition(|c| matches!(c, ActorCmd::Think { .. }));
        for (i, cmd) in queue.into_iter().enumerate() {
            match cmd {
                ActorCmd::Think { .. } if Some(i) != newest => {}
                ActorCmd::Think { request, moves, tc, reserves } => {
                    let mut ready = Ok(());
                    if let Some(old) = thinking.take() {
                        ready = finish_search(&mut engine, old, stop_sent, &report).await;
                    }
                    if ready.is_ok() {
                        ready = prepare(&mut engine, &mut told, moves, tc, reserves).await;
                    }
                    if ready.is_ok() {
                        ready = engine.go().await;
                    }
                    if let Err(e) = ready {
                        report(request, ActorEvent::Failed(format!("{} failed: {e}", spec.name)));
                        return;
                    }
                    thinking = Some(request);
                    stop_sent = false;
                }
                ActorCmd::Stop => {
                    if thinking.is_some() && !stop_sent {
                        engine.stop().await.ok();
                        stop_sent = true;
                    }
                }
            }
        }
    }
}

/// Sends the options for `purpose`: for analysis, the profile's (unless the
/// user set the same option), then the user's own.
async fn send_options(
    engine: &mut Engine,
    spec: &EngineSpec,
    profile: Profile,
    purpose: Purpose,
) -> Result<(), AeiError> {
    if purpose == Purpose::Analysis {
        for &(name, value) in profile.analysis_options() {
            if !spec.options.iter().any(|o| o.name.eq_ignore_ascii_case(name)) {
                engine.set_option(name, value).await?;
            }
        }
    }
    for o in &spec.options {
        engine.set_option(&o.name, &o.value).await?;
    }
    Ok(())
}

/// Stops the search for `request` and waits for its `bestmove`, passing on
/// what the engine says meanwhile.
async fn finish_search(
    engine: &mut Engine,
    request: u64,
    stop_sent: bool,
    report: &impl Fn(u64, ActorEvent),
) -> Result<(), AeiError> {
    if !stop_sent {
        engine.stop().await?;
    }
    let deadline = tokio::time::Instant::now() + STOP_TIMEOUT;
    loop {
        match engine.recv_until(deadline).await? {
            None => return Err(AeiError::Timeout("bestmove after stop")),
            Some(EngineMessage::BestMove(text)) => {
                report(request, ActorEvent::BestMove(text));
                return Ok(());
            }
            Some(other) => report(request, ActorEvent::Line(other)),
        }
    }
}

/// Brings the engine up to date: a fresh `newgame` (with the time control)
/// if the move list isn't an extension of what it was told, then any new
/// moves, then the current clock.
async fn prepare(
    engine: &mut Engine,
    told: &mut Option<Vec<String>>,
    moves: Vec<String>,
    tc: Option<TimeControl>,
    reserves: Option<[Duration; 2]>,
) -> Result<(), AeiError> {
    let known = match told {
        Some(t) if moves.starts_with(t) => t.len(),
        _ => {
            if let Some(tc) = &tc {
                engine.set_time_control(tc).await?;
            }
            engine.new_game().await?;
            engine.is_ready(Duration::from_secs(30)).await?;
            0
        }
    };
    for m in &moves[known..] {
        engine.make_move(m).await?;
    }
    *told = Some(moves);
    if let Some(r) = reserves {
        engine.set_clock(r).await?;
    }
    Ok(())
}

/// A search log's depth to one decimal place, with `+` while an iteration
/// is unfinished (`12.0233+` -> `12.0+`, `+3.001` -> `3.0+`).
fn log_depth(depth: &str) -> String {
    let plus = if depth.contains('+') { "+" } else { "" };
    let digits = depth.trim_matches('+');
    match digits.parse::<f64>() {
        Ok(d) if d.fract() != 0.0 => format!("{d:.1}{plus}"),
        _ => format!("{digits}{plus}"),
    }
}

/// One engine message, read for display and for what it says about the
/// search (each field present only if the message carries it).
struct Search {
    kind: EngineOutputKind,
    /// Display text, e.g. `depth 12+` or the log line.
    text: String,
    depth: Option<String>,
    /// From the mover's side.
    score: Option<Score>,
    pv: Option<Vec<String>>,
    nodes: Option<u64>,
    /// Seconds.
    time: Option<f64>,
}

impl Search {
    /// Whether it says anything about the search.
    fn progress(&self) -> bool {
        self.depth.is_some()
            || self.score.is_some()
            || self.pv.is_some()
            || self.nodes.is_some()
            || self.time.is_some()
    }
}

/// Reads an engine message as the engine's profile says. `None` for
/// messages that aren't worth showing.
fn read_search(profile: Profile, msg: EngineMessage) -> Option<Search> {
    let mut out = Search {
        kind: EngineOutputKind::Info,
        text: String::new(),
        depth: None,
        score: None,
        pv: None,
        nodes: None,
        time: None,
    };
    match msg {
        EngineMessage::Info(info) => {
            out.text = match info {
                Info::Depth { depth, in_progress } => {
                    let d = format!("{depth}{}", if in_progress { "+" } else { "" });
                    out.depth = Some(d.clone());
                    format!("depth {d}")
                }
                Info::Score(s) => {
                    out.score = Some(profile.info_score(s));
                    format!("score {s}")
                }
                Info::Pv(turns) => {
                    let text = format!("pv {}", turns.join(" | "));
                    out.pv = Some(turns);
                    text
                }
                Info::Nodes(n) => {
                    out.nodes = Some(n);
                    format!("nodes {n}")
                }
                Info::Time(t) => {
                    out.time = Some(t);
                    format!("time {t}")
                }
                Info::CurrMoveNumber(n) => format!("currmovenumber {n}"),
                Info::Other { key, value } => format!("{key} {value}"),
            };
        }
        EngineMessage::Log(text) => {
            // Sharp's verbose output has a blank line after each iteration.
            if text.is_empty() {
                return None;
            }
            if profile.logs_search()
                && let Some(search) = SearchLog::parse(&text)
            {
                out.depth = Some(log_depth(&search.depth));
                out.score = Some(profile.log_score(&search.eval));
                out.pv = Some(search.pv).filter(|pv| !pv.is_empty());
                out.time = search.time;
            }
            out.kind = EngineOutputKind::Log;
            out.text = text;
        }
        EngineMessage::Unknown(line) => {
            out.kind = EngineOutputKind::Unexpected;
            out.text = line;
        }
        EngineMessage::ReadyOk | EngineMessage::AeiOk => return None,
        other => {
            out.kind = EngineOutputKind::Unexpected;
            out.text = format!("{other:?}");
        }
    }
    Some(out)
}

/// A match engine's message for its panel. Scores are its own view, in
/// centi-rabbits; a proven result has none (the line still shows it).
fn engine_output(side: Color, profile: Profile, msg: EngineMessage) -> Option<EngineOutput> {
    let s = read_search(profile, msg)?;
    Some(EngineOutput {
        side,
        kind: s.kind,
        text: s.text,
        depth: s.depth,
        score: s.score.and_then(Score::centi_rabbits),
        pv: s.pv,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::EventSink;
    use crate::dto::EngineOption;
    use serde_json::Value;

    #[test]
    fn sharps_search_log_fills_depth_and_score() {
        let log =
            |text: &str| engine_output(Color::Gold, Profile::Sharp, EngineMessage::Log(text.into())).unwrap();
        let out = log("Depth 12.0233+ Eval -1078 Time 2.8 Seed 47d0b298fc1972e4");
        assert_eq!(
            (out.kind, out.depth.as_deref(), out.score),
            (EngineOutputKind::Log, Some("12.0+"), Some(-107))
        );
        assert_eq!(out.text, "Depth 12.0233+ Eval -1078 Time 2.8 Seed 47d0b298fc1972e4");
        let won = log("Depth 7 Eval 999990 Time 0.1");
        assert_eq!((won.depth.as_deref(), won.score), (Some("7"), None));
        let id = log("ID Depth:  12     Eval:     90 Time: 2.37/4.89  PV: Ee2n Ee3n Ee4n Ee5n  ed7s qpss");
        assert_eq!((id.depth.as_deref(), id.score), (Some("12"), Some(9)));
        assert_eq!(id.pv.unwrap(), ["Ee2n Ee3n Ee4n Ee5n", "ed7s"]);
        let fs = log("FS Depth: +3.001  Eval:    158 Time: 0.02/8.21  PV: Ee2n Ee3n Ee4n Ee5n  dg7s qpss");
        assert_eq!(fs.depth.as_deref(), Some("3.0+"));
        assert!(engine_output(Color::Gold, Profile::Sharp, EngineMessage::Log(String::new())).is_none());
        let plain = log("Started new game");
        assert_eq!((plain.depth, plain.score), (None, None));
        // Other engines' logs are only logs.
        let other = engine_output(Color::Gold, Profile::Generic, EngineMessage::Log("Depth 7 Eval 3".into()))
            .unwrap();
        assert_eq!((other.depth, other.score), (None, None));
    }

    #[test]
    fn evals_are_from_golds_side() {
        assert_eq!(eval_for_gold(Score::CentiRabbits(40), Color::Silver), Eval::CentiRabbits { value: -40 });
        assert_eq!(eval_for_gold(Score::Win, Color::Silver), Eval::Decided { winner: Color::Silver });
        assert_eq!(eval_for_gold(Score::Win, Color::Gold), Eval::Decided { winner: Color::Gold });
    }

    #[test]
    fn pv_is_checked_turn_by_turn() {
        let mut game = Game::new();
        game.play_notation(SETUPS[0]).unwrap();
        game.play_notation(SETUPS[1]).unwrap();
        let pv: Vec<String> =
            ["Ee2n Ee3n Ee4n Ee5n", "ed7s", "Ee6s", "Ee5n"].iter().map(|t| t.to_string()).collect();
        let (turns, cut) = validate_pv(&game, &pv);
        assert_eq!(turns.iter().map(|t| t.label.as_str()).collect::<Vec<_>>(), ["2g", "2s", "3g"]);
        assert_eq!(turns[1].steps.as_ref().unwrap().color, Color::Silver);
        assert_eq!(cut.as_deref(), Some("Ee5n"), "silver can't move gold's elephant alone");
    }

    /// The bundled test engine, built in the test binary's parent directory.
    fn test_engine(args: &[&str]) -> EngineSpec {
        let exe = std::env::current_exe().unwrap();
        let dir = exe.parent().and_then(|d| d.parent()).unwrap();
        let program = dir.join(if cfg!(windows) { "aei-test-engine.exe" } else { "aei-test-engine" });
        assert!(
            program.exists(),
            "{} is missing: run `cargo build -p arimaa-aei --bin aei-test-engine`",
            program.display()
        );
        EngineSpec {
            id: String::new(),
            name: "Test engine".into(),
            program: program.display().to_string(),
            args: args.iter().map(|a| a.to_string()).collect(),
            working_dir: None,
            options: vec![EngineOption { name: "threads".into(), value: "2".into() }],
        }
    }

    const SETUPS: [&str; 2] = [
        "Ra1 Rb1 Rc1 Rd1 Re1 Rf1 Rg1 Rh1 Ca2 Db2 Hc2 Md2 Ee2 Hf2 Dg2 Ch2",
        "ra8 rb8 rc8 rd8 re8 rf8 rg8 rh8 ca7 db7 hc7 ed7 me7 hf7 dg7 ch7",
    ];

    /// Messages from an actor, up to the first that `stop` picks.
    async fn collect(
        rx: &mut UnboundedReceiver<ControlMsg>,
        mut stop: impl FnMut(u64, &ActorEvent) -> bool,
    ) -> Vec<(u64, ActorEvent)> {
        let mut seen = Vec::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        loop {
            let msg = tokio::time::timeout_at(deadline, rx.recv()).await.expect("actor went quiet").unwrap();
            let ControlMsg::FromActor { request, event, .. } = msg else { continue };
            let done = stop(request, &event);
            seen.push((request, event));
            if done {
                return seen;
            }
        }
    }

    fn logs(seen: &[(u64, ActorEvent)]) -> Vec<String> {
        seen.iter()
            .filter_map(|(_, e)| match e {
                ActorEvent::Line(EngineMessage::Log(text)) => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    fn start(purpose: Purpose) -> (UnboundedSender<ActorCmd>, UnboundedReceiver<ControlMsg>) {
        let (cmd_tx, cmd_rx) = unbounded_channel();
        let (tx, rx) = unbounded_channel();
        let spec = test_engine(&["--until-stop"]);
        tokio::spawn(run_actor(spec, purpose, Origin::Analysis { run: 1 }, cmd_rx, tx));
        (cmd_tx, rx)
    }

    #[tokio::test]
    async fn options_go_after_the_handshake() {
        let (_cmds, mut rx) = start(Purpose::Analysis);
        let seen = collect(&mut rx, |_, e| matches!(e, ActorEvent::Started { .. })).await;
        assert_eq!(logs(&seen), ["option threads=2"]);
        assert!(matches!(seen.last().unwrap().1, ActorEvent::Started { profile: Profile::Generic, .. }));
    }

    #[tokio::test]
    async fn a_new_search_waits_for_the_old_ones_bestmove() {
        let (cmds, mut rx) = start(Purpose::Analysis);
        collect(&mut rx, |_, e| matches!(e, ActorEvent::Started { .. })).await;
        let moves: Vec<String> = SETUPS.iter().map(|s| s.to_string()).collect();
        let think = |request, n: usize| ActorCmd::Think {
            request,
            moves: moves[..n].to_vec(),
            tc: None,
            reserves: None,
        };
        let is_pv = |e: &ActorEvent| matches!(e, ActorEvent::Line(EngineMessage::Info(Info::Pv(_))));
        cmds.send(think(1, 1)).unwrap();
        let mut seen = collect(&mut rx, |_, e| is_pv(e)).await;
        // Mid-search: the test engine logs anything but `stop` as an error.
        cmds.send(think(2, 2)).unwrap();
        cmds.send(think(3, 1)).unwrap();
        cmds.send(think(4, 2)).unwrap();
        seen.extend(collect(&mut rx, |r, e| r == 4 && is_pv(e)).await);
        assert!(logs(&seen).iter().all(|l| !l.contains("Error")), "{:?}", logs(&seen));
        assert!(seen.iter().any(|(r, e)| *r == 1 && matches!(e, ActorEvent::BestMove(_))), "1 answered");
        assert!(seen.iter().all(|(r, _)| *r != 2 && *r != 3), "overtaken searches never start");
    }

    struct Recorder(Mutex<Vec<(String, Value)>>);
    impl EventSink for Recorder {
        fn emit(&self, event: &str, payload: Value) {
            lock(&self.0).push((event.to_string(), payload));
        }
    }

    #[tokio::test]
    async fn fast_navigation_only_reports_the_last_node() {
        let path = std::env::temp_dir().join(format!("arimaa-analysis-test-{}.json", std::process::id()));
        let mut registry = EngineRegistry::load(path.clone());
        let spec = registry.save(test_engine(&["--until-stop"])).unwrap();
        let _ = std::fs::remove_file(&path);
        let session = Arc::new(Mutex::new(Session::new()));
        let recorder = Arc::new(Recorder(Mutex::new(Vec::new())));
        let (controller, run) = new(recorder.clone(), session.clone(), Arc::new(Mutex::new(registry)));
        tokio::spawn(run);
        {
            let mut s = lock(&session);
            s.load(&format!("1g {}\n1s {}\n2g Ee2n Ee3n\n", SETUPS[0], SETUPS[1])).unwrap();
            s.set_analysis(Some(AnalysisEngine { id: spec.id.clone(), name: spec.name.clone() }));
        }
        controller.poke();
        tokio::time::sleep(Duration::from_millis(300)).await;
        for ply in [0, 1, 2, 3, 1, 2] {
            lock(&session).goto(ply).unwrap();
            controller.poke();
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
        let last = serde_json::to_value(lock(&session).analysis_target().unwrap().node).unwrap();
        let updates: Vec<Value> =
            lock(&recorder.0).iter().filter(|(e, _)| e == ANALYSIS_UPDATE).map(|(_, v)| v.clone()).collect();
        for u in &updates {
            if !u["line"].is_null() {
                assert_eq!(u["line"]["node"], u["node"], "a line from another node: {u}");
            }
            let log = u["log"].as_array().unwrap();
            assert!(log.iter().all(|l| !l.as_str().unwrap().contains("Error")), "{u}");
        }
        let end = updates.last().unwrap();
        assert_eq!((&end["node"], &end["state"]), (&last, &Value::from("searching")), "{end}");
        assert!(!end["line"]["pv"].as_array().unwrap().is_empty(), "{end}");
        assert_eq!(end["line"]["pv"][0]["label"], "2g");
        assert!(!end["line"]["eval"].is_null(), "{end}");

        lock(&session).set_analysis(None);
        controller.poke();
        tokio::time::sleep(Duration::from_millis(100)).await;
        let off = lock(&recorder.0).iter().rev().find(|(e, _)| e == ANALYSIS_UPDATE).unwrap().1.clone();
        assert_eq!(off["state"], "off");
    }
}
