//! View types sent to the frontend. TypeScript definitions are generated from
//! these with ts-rs (`npm run bindings`); don't hand-edit `src/lib/bindings`.

use arimaa_core::{Color, GameResult, Piece, PieceKind, Square, StepKind};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Stable identity for a piece across plies, so the UI can animate it.
pub type PieceId = u16;

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PieceView {
    pub id: PieceId,
    pub piece: Piece,
    pub square: Square,
    pub frozen: bool,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PositionView {
    pub side_to_move: Color,
    pub pieces: Vec<PieceView>,
    /// AEI short format, handy for debugging and engine hand-off.
    pub short: String,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MoveView {
    /// Ply after this move; `goto_ply(ply)` shows the position it produced.
    pub ply: usize,
    /// Move number label, e.g. `2g`.
    pub label: String,
    pub notation: String,
}

/// A piece on a square.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PieceAt {
    pub piece: Piece,
    pub square: Square,
}

/// One step of the move that produced the shown position.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LastStepView {
    /// The piece that stepped; an enemy of the mover when pushed or pulled.
    pub piece: Piece,
    pub from: Square,
    pub to: Square,
    /// A piece captured on a trap as a result of this step.
    pub captured: Option<PieceAt>,
}

/// The move that produced the shown position, for the last-move display.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LastMoveView {
    pub color: Color,
    pub steps: Vec<LastStepView>,
}

/// Pieces of each color captured so far, strongest first.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CapturedView {
    pub gold: Vec<PieceKind>,
    pub silver: Vec<PieceKind>,
}

/// What the board needs to replay the move that produced the shown position.
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MoveReplay {
    /// Pieces before the move.
    pub before: Vec<PieceView>,
    pub animation: Vec<AnimStep>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Phase {
    /// The side to move is arranging its setup.
    Setup,
    Play,
    /// The game has a result; no more moves at the end of the record.
    Over,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TurnStepView {
    pub notation: String,
    pub kind: StepKind,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TurnView {
    pub steps: Vec<TurnStepView>,
    pub steps_left: usize,
    /// Square a pushing piece must step into, if a push is under way.
    pub push_pending: Option<Square>,
    /// Why the turn can't be committed yet, or `None` if it can.
    pub commit_blocker: Option<String>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SessionView {
    pub moves: Vec<MoveView>,
    /// Ply being shown (0 = empty board, `moves.len()` = latest).
    pub ply: usize,
    pub phase: Phase,
    /// Position at `ply`, including any in-progress turn or setup draft.
    pub position: PositionView,
    /// Present while a turn is being entered.
    pub turn: Option<TurnView>,
    /// The move that produced the position at `ply`, unless it was a setup.
    pub last_move: Option<LastMoveView>,
    /// Pieces captured up to `ply`, including in the turn being entered.
    pub captured: CapturedView,
    /// Moves after `ply` on the line being shown.
    pub moves_after_cursor: usize,
    pub result: Option<GameResult>,
    pub end_marker: Option<String>,
    /// Why the game ended, when it wasn't on the board (illegal engine move,
    /// engine crash, ...).
    pub end_detail: Option<String>,
    /// Present during a match (a game with an engine or a clock).
    pub players: Option<PlayersView>,
    pub clock: Option<ClockView>,
    /// The side whose engine is thinking.
    pub thinking: Option<Color>,
    /// Whether board input is accepted now: any time outside a match; in a
    /// match, moves anywhere (as plans) but setups only on a human's turn.
    pub can_input: bool,
    /// Whether committing now plays a move in the match (a human's turn at
    /// the live position), rather than adding a variation.
    pub plays_live: bool,
    /// In a match, the ply of the live position if it's on the line being
    /// shown.
    pub live_ply: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum PlayerKind {
    Human,
    Engine,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PlayerView {
    pub kind: PlayerKind,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PlayersView {
    pub gold: PlayerView,
    pub silver: PlayerView,
}

/// One side's clock.
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SideClockView {
    pub time_control: String,
    #[ts(type = "number")]
    pub move_time_ms: u64,
    #[ts(type = "number")]
    pub reserve_ms: u64,
}

/// Clock snapshot. The UI counts down locally from `turnElapsedMs` for the
/// running side.
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ClockView {
    /// Per side; `None` for an untimed side.
    pub gold: Option<SideClockView>,
    pub silver: Option<SideClockView>,
    /// Side whose clock is running, if any.
    pub running: Option<Color>,
    /// Time used so far on the running side's turn.
    #[ts(type = "number")]
    pub turn_elapsed_ms: u64,
    /// Total time the running side may take this turn.
    #[ts(type = "number")]
    pub turn_allowance_ms: u64,
    /// Time left before the game time limit, if there is one.
    #[ts(type = "number | null")]
    pub game_remaining_ms: Option<u64>,
}

/// A player choice when starting a game.
#[derive(Clone, Debug, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export)]
pub enum PlayerSpec {
    Human,
    #[serde(rename_all = "camelCase")]
    Engine {
        engine_id: String,
    },
}

#[derive(Clone, Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MatchSpec {
    pub gold: PlayerSpec,
    pub silver: PlayerSpec,
    /// Arimaa time control per side, e.g. `30s/5m`; `None` leaves that side
    /// untimed.
    pub gold_time_control: Option<String>,
    pub silver_time_control: Option<String>,
}

/// A configured engine.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EngineSpec {
    /// Stable id; empty when adding a new engine.
    pub id: String,
    pub name: String,
    pub program: String,
    pub args: Vec<String>,
    pub working_dir: Option<String>,
}

/// What an engine reported about itself in the handshake.
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EngineIdentity {
    pub name: Option<String>,
    pub author: Option<String>,
    pub version: Option<String>,
    pub protocol_version: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum EngineOutputKind {
    /// Parsed `info` line.
    Info,
    Log,
    /// Something the engine shouldn't have sent.
    Unexpected,
    /// A controller status message (started, thinking, failed...).
    Status,
}

/// Payload of the `engine://output` event.
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EngineOutput {
    pub side: Color,
    pub kind: EngineOutputKind,
    /// Display text, e.g. `depth 12+` or the log line.
    pub text: String,
    pub depth: Option<String>,
    pub score: Option<i32>,
    pub pv: Option<Vec<String>>,
}

/// A piece appearing or disappearing during an animation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AnimPiece {
    pub id: PieceId,
    pub piece: Piece,
    pub square: Square,
}

/// One step of an animation. Play in order: fade in `restored`, slide `id`
/// from `from` to `to`, then fade out `captured`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AnimStep {
    pub id: PieceId,
    pub from: Square,
    pub to: Square,
    pub captured: Option<AnimPiece>,
    pub restored: Option<AnimPiece>,
}

impl AnimStep {
    /// The same step played backward.
    pub fn reversed(self) -> AnimStep {
        AnimStep { id: self.id, from: self.to, to: self.from, captured: None, restored: self.captured }
    }
}

/// Payload of the `game://changed` event: the new state, plus the steps to
/// animate from the previous state (empty means snap).
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SessionUpdate {
    pub view: SessionView,
    pub animation: Vec<AnimStep>,
    /// For a live move: how long it took off the clock. Its animation
    /// shouldn't take longer than this.
    #[ts(type = "number | null")]
    pub animation_budget_ms: Option<u64>,
}

#[derive(Clone, Copy, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StepTarget {
    pub to: Square,
    /// `None` for setup swaps.
    pub kind: Option<StepKind>,
    /// Steps the piece needs to get there (more than 1 for a multi-step route).
    pub steps: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ErrorKind {
    /// A game record failed to parse or replay.
    Record,
    /// The requested step or move isn't legal.
    Illegal,
    /// The request doesn't fit the current state (e.g. a step during setup).
    State,
}

/// Error returned by commands.
#[derive(Clone, Debug, Serialize, TS, thiserror::Error)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
#[error("{message}")]
pub struct ApiError {
    pub kind: ErrorKind,
    pub message: String,
    /// 1-based record line, for `Record` errors.
    pub line: Option<usize>,
}

impl ApiError {
    pub fn new(kind: ErrorKind, message: impl ToString) -> ApiError {
        ApiError { kind, message: message.to_string(), line: None }
    }

    pub fn illegal(message: impl ToString) -> ApiError {
        ApiError::new(ErrorKind::Illegal, message)
    }

    pub fn state(message: impl ToString) -> ApiError {
        ApiError::new(ErrorKind::State, message)
    }
}

impl From<arimaa_core::RecordError> for ApiError {
    fn from(e: arimaa_core::RecordError) -> ApiError {
        ApiError { kind: ErrorKind::Record, message: e.error.to_string(), line: Some(e.line) }
    }
}
