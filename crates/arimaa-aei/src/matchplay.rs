//! Playing a full game between two engines, with clocks. Follows AEI's
//! `pyrimaa/game.py`: the same options are sent, the same timeouts apply,
//! and every move is validated by `arimaa-core` before it reaches the other
//! engine.

use std::time::Duration;

use arimaa_core::{Color, Game, GameResult, TimeControl, WinReason, limit_score_winner};
use tokio::time::Instant;

use crate::engine::{Engine, EngineId};
use crate::error::AeiError;
use crate::message::{EngineMessage, Info};

#[derive(Clone, Debug)]
pub struct MatchConfig {
    /// `None` plays without clocks.
    pub time_control: Option<TimeControl>,
    /// Send `stop` this long before a turn's deadline, so a slow engine
    /// still gets a move in.
    pub stop_margin: Duration,
    pub ready_timeout: Duration,
}

impl Default for MatchConfig {
    fn default() -> Self {
        MatchConfig {
            time_control: None,
            stop_margin: Duration::from_secs(1),
            ready_timeout: Duration::from_secs(30),
        }
    }
}

/// Progress reports from [`play_match`].
#[derive(Clone, Debug, PartialEq)]
pub enum MatchEvent {
    Started {
        gold: EngineId,
        silver: EngineId,
    },
    TurnStarted {
        ply: usize,
        side: Color,
        reserve: Option<Duration>,
        allowance: Option<Duration>,
    },
    Info {
        side: Color,
        info: Info,
    },
    Log {
        side: Color,
        text: String,
    },
    /// A line the controller didn't expect (unknown message type, or a
    /// known one at the wrong time).
    Unexpected {
        side: Color,
        line: String,
    },
    MovePlayed {
        ply: usize,
        side: Color,
        notation: String,
        used: Duration,
        reserve: Option<Duration>,
    },
}

#[derive(Clone, Debug)]
pub struct MatchOutcome {
    pub game: Game,
    pub result: GameResult,
    /// Why the game ended, when it wasn't on the board (e.g. the illegal
    /// move or the engine error).
    pub detail: Option<String>,
    /// Remaining reserves (gold, silver), if the game was timed.
    pub reserves: Option<[Duration; 2]>,
}

/// Plays one game from the start. Engines must have completed the
/// handshake. They are left running afterwards, so the caller can reuse or
/// quit them. Errors are returned only for failures before the first move;
/// a failure during the game loses it for the engine that failed.
pub async fn play_match(
    gold: &mut Engine,
    silver: &mut Engine,
    config: &MatchConfig,
    mut on_event: impl FnMut(MatchEvent),
) -> Result<MatchOutcome, AeiError> {
    let mut engines = [gold, silver];
    let tc = config.time_control;
    for engine in engines.iter_mut() {
        if let Some(tc) = tc {
            engine.set_time_control(&tc).await?;
        }
        engine.new_game().await?;
        engine.is_ready(config.ready_timeout).await?;
    }
    on_event(MatchEvent::Started { gold: engines[0].id().clone(), silver: engines[1].id().clone() });

    let mut game = Game::new();
    let mut reserves = tc.map(|t| [t.starting_reserve(); 2]);
    let game_start = Instant::now();
    let game_deadline = tc.filter(|t| t.time_limit > 0).map(|t| game_start + secs(t.time_limit));
    let mut detail = None;

    while game.result().is_none() {
        let ply = game.ply_count();
        let side = game.current_position().side_to_move();
        let me = side.index();
        let engine = &mut *engines[me];

        let mut deadline = None;
        let mut allowance = None;
        if let (Some(tc), Some(res)) = (tc, reserves) {
            engine.set_clock(res).await.ok();
            let a = tc.turn_allowance(res[me]);
            allowance = Some(a);
            let d = Instant::now() + a;
            deadline = Some(game_deadline.map_or(d, |g| d.min(g)));
        }
        on_event(MatchEvent::TurnStarted { ply, side, reserve: reserves.map(|r| r[me]), allowance });

        let start = Instant::now();
        let reply = match engine.go().await {
            Ok(()) => wait_for_move(engine, side, deadline, config.stop_margin, &mut on_event).await,
            Err(e) => Err(e),
        };
        let used = start.elapsed();
        let opponent = side.opponent();

        let text = match reply {
            Err(e) => {
                detail = Some(format!("{} ({}) failed: {e}", engine.name(), side_name(side)));
                game.end_game(GameResult { winner: opponent, reason: WinReason::Forfeit }).ok();
                break;
            }
            Ok(None) => {
                let past_game_limit = game_deadline.is_some_and(|g| Instant::now() >= g);
                let result = if past_game_limit {
                    GameResult {
                        winner: limit_score_winner(game.current_position()),
                        reason: WinReason::Score,
                    }
                } else {
                    GameResult { winner: opponent, reason: WinReason::Timeout }
                };
                detail = Some(format!("{} ({}) ran out of time", engine.name(), side_name(side)));
                game.end_game(result).ok();
                break;
            }
            Ok(Some(text)) => text,
        };

        if text.trim().eq_ignore_ascii_case("resign") {
            game.end_game(GameResult { winner: opponent, reason: WinReason::Resignation }).ok();
            break;
        }
        if let Err(e) = game.play_notation(&text) {
            detail = Some(format!("{} ({}) played {text:?}: {e}", engine.name(), side_name(side)));
            game.end_game(GameResult { winner: opponent, reason: WinReason::IllegalMove }).ok();
            break;
        }
        if let (Some(tc), Some(res)) = (tc, reserves.as_mut())
            && !Game::is_setup_ply(ply)
        {
            res[me] = tc.reserve_after(res[me], used);
        }

        let notation = game.moves().last().expect("a move was just played").notation();
        for engine in engines.iter_mut() {
            // A broken engine shows up as a failure on its next turn.
            engine.make_move(&notation).await.ok();
        }
        on_event(MatchEvent::MovePlayed { ply, side, notation, used, reserve: reserves.map(|r| r[me]) });

        if let Some(tc) = tc
            && tc.turn_limit > 0
            && game.result().is_none()
            && side == Color::Silver
            && (ply / 2 + 1) as u32 >= tc.turn_limit
        {
            let winner = limit_score_winner(game.current_position());
            game.end_game(GameResult { winner, reason: WinReason::Score }).ok();
            detail = Some(format!("turn limit of {} reached", tc.turn_limit));
        }
    }

    let result = game.result().expect("the loop ends with a result");
    Ok(MatchOutcome { game, result, detail, reserves })
}

/// Reads until `bestmove`, reporting everything else. `Ok(None)` means the
/// deadline passed without a move; `stop` is sent `margin` before it.
async fn wait_for_move(
    engine: &mut Engine,
    side: Color,
    deadline: Option<Instant>,
    margin: Duration,
    on_event: &mut impl FnMut(MatchEvent),
) -> Result<Option<String>, AeiError> {
    let stop_at = deadline.map(|d| d.checked_sub(margin).unwrap_or(d));
    let mut stop_sent = false;
    loop {
        let wake = if stop_sent { deadline } else { stop_at };
        let msg = match wake {
            Some(t) => engine.recv_until(t).await?,
            None => Some(engine.recv().await?),
        };
        match msg {
            None if !stop_sent && stop_at.is_some_and(|s| Some(s) != deadline) => {
                engine.stop().await?;
                stop_sent = true;
            }
            None => return Ok(None),
            Some(EngineMessage::BestMove(text)) => return Ok(Some(text)),
            Some(EngineMessage::Info(info)) => on_event(MatchEvent::Info { side, info }),
            Some(EngineMessage::Log(text)) => on_event(MatchEvent::Log { side, text }),
            Some(EngineMessage::Unknown(line)) => on_event(MatchEvent::Unexpected { side, line }),
            Some(other) => on_event(MatchEvent::Unexpected { side, line: format!("{other:?}") }),
        }
    }
}

fn secs(s: u32) -> Duration {
    Duration::from_secs(s.into())
}

fn side_name(side: Color) -> &'static str {
    match side {
        Color::Gold => "gold",
        Color::Silver => "silver",
    }
}
