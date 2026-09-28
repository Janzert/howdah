//! Error types.

use std::fmt;

use thiserror::Error;

use crate::types::{Piece, PieceKind, Square};

/// A syntax error in notation, optionally tied to a line of a game record.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub struct ParseError {
    pub message: String,
    /// 1-based line number within a multi-line record, if known.
    pub line: Option<usize>,
}

impl ParseError {
    pub fn new(message: impl Into<String>) -> ParseError {
        ParseError { message: message.into(), line: None }
    }

    pub fn at_line(mut self, line: usize) -> ParseError {
        self.line = Some(line);
        self
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            Some(line) => write!(f, "line {line}: {}", self.message),
            None => write!(f, "{}", self.message),
        }
    }
}

/// Why a single step is not allowed.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum StepError {
    #[error("no piece on {0}")]
    NoPiece(Square),
    #[error("expected {expected} on {square}, found {found}")]
    PieceMismatch { square: Square, expected: Piece, found: Piece },
    #[error("step leaves the board")]
    OffBoard,
    #[error("{from} and {to} are not adjacent")]
    NotAdjacent { from: Square, to: Square },
    #[error("{0} is occupied")]
    Occupied(Square),
    #[error("the piece on {0} is frozen")]
    Frozen(Square),
    #[error("rabbits can't step backward")]
    RabbitBackward,
    #[error("the turn already has 4 steps")]
    TurnFull,
    #[error("the push must be completed by a stronger piece stepping into {0}")]
    PushIncomplete(Square),
    #[error("no stronger unfrozen piece can push or pull the piece on {0}")]
    CannotMoveEnemy(Square),
    #[error("not enough steps left to complete a push")]
    NoStepsForPush,
}

/// Why an in-progress turn can't be committed.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum CommitError {
    #[error("a turn must have at least one step")]
    NoSteps,
    #[error("the push must be completed before ending the turn")]
    PushIncomplete,
    #[error("a turn must change the position")]
    NoChange,
}

/// Why a setup move is invalid.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum SetupError {
    #[error("{0} belongs to the other side")]
    WrongColor(Piece),
    #[error("{0} is not on a home rank")]
    NotHomeRank(Square),
    #[error("{0} is used more than once")]
    DuplicateSquare(Square),
    #[error("expected {expected} {kind:?} pieces, found {found}")]
    WrongCount { kind: PieceKind, expected: u32, found: u32 },
}

/// A move that can't be added to a game.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum GameError {
    #[error(transparent)]
    Parse(#[from] ParseError),
    #[error("illegal step {step}: {source}")]
    Step { step: String, source: StepError },
    #[error(transparent)]
    Setup(#[from] SetupError),
    #[error(transparent)]
    Commit(#[from] CommitError),
    #[error("the game is over")]
    GameOver,
    #[error("expected a setup move")]
    ExpectedSetup,
    #[error("setup moves are only allowed at the start of the game")]
    ExpectedTurn,
    #[error("capture {0} doesn't match what happened")]
    CaptureMismatch(String),
    #[error("expected move {expected}, found {found}")]
    OutOfSequence { expected: String, found: String },
    #[error("the turn was built from a different position")]
    StaleTurn,
    #[error("the move repeats a position for the third time")]
    Repetition,
    #[error("the move has no steps or placements")]
    EmptyMove,
}

/// A [`GameError`] located on a line of a game record.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
#[error("line {line}: {error}")]
pub struct RecordError {
    pub line: usize,
    pub error: GameError,
}
