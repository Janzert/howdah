//! The setup phase: each side's first move places all 16 pieces on its
//! two home ranks.

use std::fmt;

use crate::error::SetupError;
use crate::position::Position;
use crate::types::{Color, Piece, PieceKind, Square, data_type};

data_type! {
    /// A piece placed during setup.
    pub struct Placement {
        pub piece: Piece,
        pub square: Square,
    }
}

/// Formats as standard notation, e.g. `Ra1`.
impl fmt::Display for Placement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.piece, self.square)
    }
}

/// Checks that `placements` is a complete, legal setup for `color`.
pub fn validate_setup(color: Color, placements: &[Placement]) -> Result<(), SetupError> {
    let mut used = 0u64;
    let mut counts = [0u32; 6];
    for p in placements {
        if p.piece.color != color {
            return Err(SetupError::WrongColor(p.piece));
        }
        if !color.home_ranks().contains(&p.square.rank()) {
            return Err(SetupError::NotHomeRank(p.square));
        }
        if used & p.square.bit() != 0 {
            return Err(SetupError::DuplicateSquare(p.square));
        }
        used |= p.square.bit();
        counts[p.piece.kind.index()] += 1;
    }
    for kind in PieceKind::ALL.into_iter().rev() {
        let (expected, found) = (kind.initial_count(), counts[kind.index()]);
        if found != expected {
            return Err(SetupError::WrongCount { kind, expected, found });
        }
    }
    Ok(())
}

/// Places a validated setup for the side to move and hands the move to the
/// other side.
pub fn apply_setup(pos: &Position, placements: &[Placement]) -> Result<Position, SetupError> {
    let color = pos.side_to_move();
    validate_setup(color, placements)?;
    let mut next = pos.clone();
    for p in placements {
        if next.piece_at(p.square).is_some() {
            return Err(SetupError::DuplicateSquare(p.square));
        }
        next.set(p.square, Some(p.piece));
    }
    next.set_side_to_move(color.opponent());
    Ok(next)
}

/// A conventional starting arrangement: rabbits on the back rank, the other
/// pieces in front, as a starting point for UIs to rearrange.
pub fn default_setup(color: Color) -> Vec<Placement> {
    let [back, front] = match color {
        Color::Gold => [0, 1],
        Color::Silver => [7, 6],
    };
    let front_row = "HDCMECDH";
    let mut out = Vec::with_capacity(16);
    for (file, c) in front_row.chars().enumerate() {
        let kind = Piece::from_letter(c).unwrap().kind;
        let square = Square::from_coords(file as u8, front).unwrap();
        out.push(Placement { piece: Piece::new(color, kind), square });
    }
    for file in 0..8 {
        let square = Square::from_coords(file, back).unwrap();
        out.push(Placement { piece: Piece::new(color, PieceKind::Rabbit), square });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_setups_are_valid() {
        for c in Color::ALL {
            validate_setup(c, &default_setup(c)).unwrap();
        }
    }

    #[test]
    fn apply_both_setups() {
        let p = apply_setup(&Position::empty(Color::Gold), &default_setup(Color::Gold)).unwrap();
        assert_eq!(p.side_to_move(), Color::Silver);
        let p = apply_setup(&p, &default_setup(Color::Silver)).unwrap();
        assert_eq!(p.side_to_move(), Color::Gold);
        assert_eq!(p.pieces().count(), 32);
    }

    #[test]
    fn wrong_side_setup() {
        let err = apply_setup(&Position::empty(Color::Gold), &default_setup(Color::Silver));
        assert!(matches!(err, Err(SetupError::WrongColor(_))));
    }

    #[test]
    fn setup_errors() {
        let mut s = default_setup(Color::Gold);
        s[0].square = "a3".parse().unwrap();
        assert!(matches!(validate_setup(Color::Gold, &s), Err(SetupError::NotHomeRank(_))));

        let mut s = default_setup(Color::Gold);
        s[1].square = s[0].square;
        assert!(matches!(validate_setup(Color::Gold, &s), Err(SetupError::DuplicateSquare(_))));

        let mut s = default_setup(Color::Gold);
        s[4].piece.kind = PieceKind::Rabbit; // elephant -> rabbit
        assert!(matches!(
            validate_setup(Color::Gold, &s),
            Err(SetupError::WrongCount { kind: PieceKind::Elephant, .. })
        ));

        let s = &default_setup(Color::Gold)[..15];
        assert!(validate_setup(Color::Gold, s).is_err());
    }
}
