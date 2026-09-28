//! Step and capture values.

use std::fmt;

use crate::types::{Dir, Piece, Square, data_type};

data_type! {
    /// One piece moving one square. Whether it's legal depends on the turn
    /// context; see [`crate::TurnBuilder`].
    pub struct Step {
        pub piece: Piece,
        pub from: Square,
        pub dir: Dir,
    }
}

impl Step {
    pub fn new(piece: Piece, from: Square, dir: Dir) -> Step {
        Step { piece, from, dir }
    }

    /// Destination square, or `None` if the step would leave the board.
    pub fn to(&self) -> Option<Square> {
        self.from.neighbor(self.dir)
    }
}

/// Formats as standard notation, e.g. `Ed2n`.
impl fmt::Display for Step {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}{}", self.piece, self.from, self.dir.letter())
    }
}

data_type! {
    /// A piece removed from a trap.
    pub struct Capture {
        pub piece: Piece,
        pub square: Square,
    }
}

/// Formats as standard notation, e.g. `cf3x`.
impl fmt::Display for Capture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}x", self.piece, self.square)
    }
}

data_type! {
    /// A step that was applied, with everything needed to animate or undo it.
    ///
    /// A step can capture at most one piece, and it's always a piece of the
    /// stepping piece's color: either the stepping piece itself on a trap, or a
    /// friend left alone on a trap next to the square it vacated. No square is
    /// adjacent to two traps, so both can't happen at once.
    pub struct StepEffect {
        pub step: Step,
        pub to: Square,
        pub capture: Option<Capture>,
    }
}

/// Formats as the step plus its capture token, e.g. `Rc4s Rc3x`.
impl fmt::Display for StepEffect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.step)?;
        if let Some(c) = self.capture {
            write!(f, " {c}")?;
        }
        Ok(())
    }
}
