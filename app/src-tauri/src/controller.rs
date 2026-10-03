//! Background game controller: runs engine players and clocks.
//!
//! One coordinator task owns the engine actors (one task per engine
//! process). It watches the session for an engine to move, asks that
//! engine's actor to think, sends `stop` shortly before the deadline, flags
//! timeouts, and applies replies through the session. The session stays the
//! only place game state changes, and every change goes out as
//! `game://changed`. Engine search output goes out as `engine://output`.
//!
//! Commands call [`Controller::poke`] after changing the session, so the
//! coordinator re-checks whose turn it is.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use arimaa_aei::{Engine, EngineMessage, Info, SearchLog};
use arimaa_core::{Color, TimeControl};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::backend::{Events, emit, emit_session};
use crate::dto::{EngineOutput, EngineOutputKind, EngineSpec};
use crate::engines::{EngineRegistry, engine_config};
use crate::session::Session;

/// Event carrying an [`EngineOutput`].
pub const ENGINE_OUTPUT: &str = "engine://output";

/// Longest `stop` is sent before a deadline.
const STOP_MARGIN: Duration = Duration::from_secs(1);

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
    /// The session changed; check whether an engine should move.
    pub fn poke(&self) {
        let _ = self.tx.send(ControlMsg::Poke);
    }

    /// Ask the thinking engine to move now (sends `stop`).
    pub fn move_now(&self) {
        let _ = self.tx.send(ControlMsg::MoveNow);
    }
}

enum ControlMsg {
    Poke,
    MoveNow,
    FromActor { generation: u64, side: Color, request: u64, event: ActorEvent },
}

enum ActorEvent {
    Line(EngineMessage),
    Started(String),
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
}

struct Pending {
    request: u64,
    side: Color,
    ply: usize,
    stop_at: Option<Instant>,
    stop_sent: bool,
}

pub fn spawn(events: Events, session: SharedSession, registry: SharedRegistry) -> Controller {
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
    };
    tauri::async_runtime::spawn(coordinator.run(rx));
    Controller { tx }
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

    /// Brings actors and the pending request in line with the session.
    fn sync(&mut self) {
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
        tauri::async_runtime::spawn(run_actor(spec, side, generation, cmd_rx, self.tx.clone()));
        Some(ActorHandle { engine_id: engine_id.to_string(), cmd: cmd_tx })
    }

    fn next_timer(&self) -> Option<Instant> {
        let stop = self.pending.as_ref().filter(|p| !p.stop_sent).and_then(|p| p.stop_at);
        let deadline = lock(&self.session).turn_deadline();
        match (stop, deadline) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    fn on_timer(&mut self) {
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
            ControlMsg::FromActor { generation, side, request, event } => {
                if self.generation != Some(generation) {
                    return;
                }
                self.on_actor_event(generation, side, request, event);
            }
        }
    }

    fn on_actor_event(&mut self, generation: u64, side: Color, request: u64, event: ActorEvent) {
        match event {
            ActorEvent::Line(msg) => {
                if let Some(out) = engine_output(side, msg) {
                    emit(&self.events, ENGINE_OUTPUT, out);
                }
            }
            ActorEvent::Started(name) => self.output(side, EngineOutputKind::Status, format!("{name} ready")),
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
}

async fn sleep_until(t: Option<Instant>) {
    if let Some(t) = t {
        tokio::time::sleep_until(tokio::time::Instant::from_std(t)).await;
    }
}

/// One engine process. Keeps the engine's view of the game in sync with the
/// move list it's given, thinks on request, and reports everything back.
async fn run_actor(
    spec: EngineSpec,
    side: Color,
    generation: u64,
    mut cmds: UnboundedReceiver<ActorCmd>,
    out: UnboundedSender<ControlMsg>,
) {
    let report = |request: u64, event: ActorEvent| {
        let _ = out.send(ControlMsg::FromActor { generation, side, request, event });
    };
    let mut engine = match Engine::start(&engine_config(&spec)).await {
        Ok(e) => e,
        Err(e) => {
            report(0, ActorEvent::Failed(format!("{} failed to start: {e}", spec.name)));
            return;
        }
    };
    report(0, ActorEvent::Started(engine.name().to_string()));
    // Moves the engine has been told, or None before its first `newgame`.
    let mut told: Option<Vec<String>> = None;
    let mut thinking: Option<u64> = None;
    loop {
        tokio::select! {
            cmd = cmds.recv() => match cmd {
                None => {
                    engine.quit(Duration::from_secs(2)).await.ok();
                    return;
                }
                Some(ActorCmd::Think { request, moves, tc, reserves }) => {
                    let ready = prepare(&mut engine, &mut told, moves, tc, reserves).await;
                    match ready.and(engine.go().await) {
                        Ok(()) => thinking = Some(request),
                        Err(e) => {
                            report(request, ActorEvent::Failed(format!("{} failed: {e}", spec.name)));
                            return;
                        }
                    }
                }
                Some(ActorCmd::Stop) => {
                    if thinking.is_some() {
                        engine.stop().await.ok();
                    }
                }
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
            }
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
) -> Result<(), arimaa_aei::AeiError> {
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

/// A search log's depth to one decimal place (`12.0233+` -> `12.0+`).
fn log_depth(depth: &str) -> String {
    let plus = if depth.ends_with('+') { "+" } else { "" };
    match depth.trim_end_matches('+').parse::<f64>() {
        Ok(d) if d.fract() != 0.0 => format!("{d:.1}{plus}"),
        _ => depth.to_string(),
    }
}

/// A search log's eval in centi-rabbits. Only bot_Sharp writes these
/// logs, and its scale is about 10 per centi-rabbit. A won or lost
/// position (near ±1,000,000) has no score; the log line still shows it.
fn log_score(eval: i32) -> Option<i32> {
    const SHARP_PER_CENTI_RABBIT: i32 = 10;
    const SHARP_DECIDED: i32 = 990_000;
    (eval.abs() < SHARP_DECIDED).then_some(eval / SHARP_PER_CENTI_RABBIT)
}

fn engine_output(side: Color, msg: EngineMessage) -> Option<EngineOutput> {
    let mut out = EngineOutput {
        side,
        kind: EngineOutputKind::Info,
        text: String::new(),
        depth: None,
        score: None,
        pv: None,
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
                    out.score = Some(s);
                    format!("score {s}")
                }
                Info::Pv(turns) => {
                    let text = format!("pv {}", turns.join(" | "));
                    out.pv = Some(turns);
                    text
                }
                Info::Nodes(n) => format!("nodes {n}"),
                Info::Time(t) => format!("time {t}"),
                Info::CurrMoveNumber(n) => format!("currmovenumber {n}"),
                Info::Other { key, value } => format!("{key} {value}"),
            };
        }
        EngineMessage::Log(text) => {
            if let Some(search) = SearchLog::parse(&text) {
                out.depth = Some(log_depth(&search.depth));
                out.score = log_score(search.eval);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sharps_search_log_fills_depth_and_score() {
        let log = |text: &str| engine_output(Color::Gold, EngineMessage::Log(text.into())).unwrap();
        let out = log("Depth 12.0233+ Eval -1078 Time 2.8 Seed 47d0b298fc1972e4");
        assert_eq!(
            (out.kind, out.depth.as_deref(), out.score),
            (EngineOutputKind::Log, Some("12.0+"), Some(-107))
        );
        assert_eq!(out.text, "Depth 12.0233+ Eval -1078 Time 2.8 Seed 47d0b298fc1972e4");
        let won = log("Depth 7 Eval 999990 Time 0.1");
        assert_eq!((won.depth.as_deref(), won.score), (Some("7"), None));
        let plain = log("Started new game");
        assert_eq!((plain.depth, plain.score), (None, None));
    }
}
