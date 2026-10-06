//! Arimaa rules, position representation and notation.
//!
//! This crate has no UI dependencies. It backs the desktop app and is meant
//! to back a headless CLI and Python bindings later.
//!
//! - [`Position`]: pieces on the board and the side to move, with raw step
//!   mechanics ([`Position::apply_step`], [`Position::undo_step`]).
//! - [`TurnBuilder`]: builds a turn step by step with full legality checks
//!   (freezing, pushes, pulls, rabbit direction, the 4-step limit).
//! - [`Game`]: a sequence of setup and turn moves, parsed from and formatted
//!   to the standard record format.
//! - [`GameTree`]: moves with variations, comments and glyphs.
//! - [`GameRecord`]: a tree with its tags, read from and written to the
//!   record format (PGN-style tags, variations and comments).
//!
//! Optional features: `serde` adds serde derives to the data types; `ts` adds
//! `ts_rs::TS` derives for generating TypeScript bindings.

mod error;
mod game;
pub mod notation;
mod outcome;
mod position;
mod record;
mod setup;
mod step;
mod timecontrol;
mod tree;
mod turn;
mod types;

pub use error::{CommitError, GameError, ParseError, RecordError, SetupError, StepError};
pub use game::{Game, Move};
pub use outcome::{
    GameResult, WinReason, is_immobilized, limit_score_winner, outcome_after_turn, outcome_with_history,
};
pub use position::Position;
pub use record::GameRecord;
pub use setup::{Placement, apply_setup, default_setup, validate_setup};
pub use step::{Capture, Step, StepEffect};
pub use timecontrol::TimeControl;
pub use tree::{Annotation, GameTree, Glyph, Node, NodeId, split_commands};
pub use turn::{MAX_STEPS, Route, StepKind, Turn, TurnBuilder, TurnStep};
pub use types::{Color, Dir, Piece, PieceKind, Square, TRAPS};
