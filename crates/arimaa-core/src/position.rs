//! Board position as bitboards, plus raw step mechanics (moving pieces and
//! resolving traps). Legality lives in [`crate::TurnBuilder`].

use std::fmt;

use crate::error::{ParseError, StepError};
use crate::step::{Capture, Step, StepEffect};
use crate::types::{Color, Piece, PieceKind, Square, TRAPS};

const FILE_A: u64 = 0x0101_0101_0101_0101;
const FILE_H: u64 = FILE_A << 7;
const TRAP_BITS: u64 = TRAPS[0].bit() | TRAPS[1].bit() | TRAPS[2].bit() | TRAPS[3].bit();

/// All squares orthogonally adjacent to any square in `bb`.
pub(crate) const fn neighbors(bb: u64) -> u64 {
    ((bb & !FILE_H) << 1) | ((bb & !FILE_A) >> 1) | (bb << 8) | (bb >> 8)
}

pub(crate) fn squares(mut bb: u64) -> impl Iterator<Item = Square> {
    std::iter::from_fn(move || {
        if bb == 0 {
            return None;
        }
        let i = bb.trailing_zeros();
        bb &= bb - 1;
        Some(Square::from_bit_index(i))
    })
}

// Zobrist keys, generated at compile time with splitmix64 so they're stable
// across builds and platforms (hashes can be persisted or compared later).
const fn splitmix64(state: u64) -> (u64, u64) {
    let state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    (state, z ^ (z >> 31))
}

struct ZobristKeys {
    pieces: [[[u64; 64]; 6]; 2],
    silver_to_move: u64,
}

const ZOBRIST: ZobristKeys = {
    let mut pieces = [[[0u64; 64]; 6]; 2];
    let mut state = 0x4172_696D_6161_u64; // "Arimaa"
    let mut c = 0;
    while c < 2 {
        let mut k = 0;
        while k < 6 {
            let mut s = 0;
            while s < 64 {
                let (next, key) = splitmix64(state);
                state = next;
                pieces[c][k][s] = key;
                s += 1;
            }
            k += 1;
        }
        c += 1;
    }
    let (_, silver_to_move) = splitmix64(state);
    ZobristKeys { pieces, silver_to_move }
};

fn piece_key(piece: Piece, sq: Square) -> u64 {
    ZOBRIST.pieces[piece.color.index()][piece.kind.index()][sq.index() as usize]
}

/// A board position and the side to move.
///
/// Pieces are stored as cumulative strength bitboards: `at_least[c][k]` holds
/// every piece of color `c` with strength `k` or greater. Arimaa mostly asks
/// "which pieces are stronger (or weaker) than this one?" (freezing, pushes,
/// pulls), and those are single lookups here. Index 0 (rabbit) is the side's
/// occupancy, and index 6 is always empty, so "stronger than an elephant"
/// needs no special case. A single piece type is `at_least[k] & !at_least[k + 1]`.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Position {
    at_least: [[u64; 7]; 2],
    side: Color,
    /// Zobrist hash of the pieces only; the side to move is mixed in by [`Position::hash`].
    board_hash: u64,
}

impl Position {
    /// An empty board with `side` to move.
    pub fn empty(side: Color) -> Position {
        Position { at_least: [[0; 7]; 2], side, board_hash: 0 }
    }

    /// Builds a position from `(piece, square)` pairs.
    pub fn from_pieces(
        side: Color,
        pieces: impl IntoIterator<Item = (Piece, Square)>,
    ) -> Result<Position, ParseError> {
        let mut pos = Position::empty(side);
        for (piece, sq) in pieces {
            if pos.piece_at(sq).is_some() {
                return Err(ParseError::new(format!("{sq} is occupied twice")));
            }
            pos.put(piece, sq);
        }
        Ok(pos)
    }

    pub fn side_to_move(&self) -> Color {
        self.side
    }

    pub fn set_side_to_move(&mut self, side: Color) {
        self.side = side;
    }

    /// Hash of the pieces and side to move (for repetition detection).
    pub fn hash(&self) -> u64 {
        match self.side {
            Color::Gold => self.board_hash,
            Color::Silver => self.board_hash ^ ZOBRIST.silver_to_move,
        }
    }

    /// True if both positions have the same pieces on the same squares,
    /// regardless of side to move.
    pub fn same_board(&self, other: &Position) -> bool {
        self.board_hash == other.board_hash && self.at_least == other.at_least
    }

    pub fn piece_at(&self, sq: Square) -> Option<Piece> {
        let bit = sq.bit();
        let color = Color::ALL.into_iter().find(|c| self.at_least[c.index()][0] & bit != 0)?;
        // The piece's strength is the last board (from rabbit up) that contains it.
        let boards = &self.at_least[color.index()];
        let strength = (1..6).take_while(|&k| boards[k] & bit != 0).count();
        Some(Piece::new(color, PieceKind::ALL[strength]))
    }

    /// Every piece on the board, ordered by square.
    pub fn pieces(&self) -> impl Iterator<Item = (Square, Piece)> + '_ {
        squares(self.occupied()).map(|sq| (sq, self.piece_at(sq).expect("occupied square has a piece")))
    }

    /// Pieces of exactly this type.
    pub fn bitboard(&self, piece: Piece) -> u64 {
        let (c, k) = (piece.color.index(), piece.kind.index());
        self.at_least[c][k] & !self.at_least[c][k + 1]
    }

    pub fn occupied_by(&self, color: Color) -> u64 {
        self.at_least[color.index()][0]
    }

    pub fn occupied(&self) -> u64 {
        self.at_least[0][0] | self.at_least[1][0]
    }

    pub fn count(&self, piece: Piece) -> u32 {
        self.bitboard(piece).count_ones()
    }

    /// `color`'s pieces with strength `kind` or greater.
    pub fn at_least(&self, color: Color, kind: PieceKind) -> u64 {
        self.at_least[color.index()][kind.index()]
    }

    /// `color`'s pieces strictly stronger than `kind`.
    pub fn stronger_than(&self, color: Color, kind: PieceKind) -> u64 {
        self.at_least[color.index()][kind.index() + 1]
    }

    /// `color`'s pieces strictly weaker than `kind`.
    pub fn weaker_than(&self, color: Color, kind: PieceKind) -> u64 {
        let boards = &self.at_least[color.index()];
        boards[0] & !boards[kind.index()]
    }

    /// True if the piece on `sq` is next to a stronger enemy and has no friend
    /// next to it. Empty squares are never frozen.
    pub fn is_frozen(&self, sq: Square) -> bool {
        let Some(piece) = self.piece_at(sq) else { return false };
        let adj = neighbors(sq.bit());
        adj & self.occupied_by(piece.color) == 0
            && adj & self.stronger_than(piece.color.opponent(), piece.kind) != 0
    }

    /// Places `piece` on `sq`, replacing whatever was there.
    pub fn set(&mut self, sq: Square, piece: Option<Piece>) {
        if let Some(old) = self.piece_at(sq) {
            self.remove(old, sq);
        }
        if let Some(p) = piece {
            self.put(p, sq);
        }
    }

    /// The boards a piece of this type appears on: rabbit up to its own strength.
    fn boards_mut(&mut self, piece: Piece) -> &mut [u64] {
        &mut self.at_least[piece.color.index()][..=piece.kind.index()]
    }

    fn put(&mut self, piece: Piece, sq: Square) {
        debug_assert!(self.occupied() & sq.bit() == 0);
        for b in self.boards_mut(piece) {
            *b |= sq.bit();
        }
        self.board_hash ^= piece_key(piece, sq);
    }

    fn remove(&mut self, piece: Piece, sq: Square) {
        debug_assert!(self.bitboard(piece) & sq.bit() != 0);
        for b in self.boards_mut(piece) {
            *b &= !sq.bit();
        }
        self.board_hash ^= piece_key(piece, sq);
    }

    /// Moves a piece between two squares; `to` must be empty.
    fn shift(&mut self, piece: Piece, from: Square, to: Square) {
        debug_assert!(self.bitboard(piece) & from.bit() != 0 && self.occupied() & to.bit() == 0);
        let bits = from.bit() | to.bit();
        for b in self.boards_mut(piece) {
            *b ^= bits;
        }
        self.board_hash ^= piece_key(piece, from) ^ piece_key(piece, to);
    }

    /// Moves a piece one square and resolves traps. Only checks the mechanics
    /// (right piece, destination on the board and empty), not whether the
    /// step is legal in the current turn.
    pub fn apply_step(&mut self, step: Step) -> Result<StepEffect, StepError> {
        let found = self.piece_at(step.from).ok_or(StepError::NoPiece(step.from))?;
        if found != step.piece {
            return Err(StepError::PieceMismatch { square: step.from, expected: step.piece, found });
        }
        let to = step.to().ok_or(StepError::OffBoard)?;
        if self.occupied() & to.bit() != 0 {
            return Err(StepError::Occupied(to));
        }
        self.shift(step.piece, step.from, to);
        let capture = self.resolve_trap(step.piece.color, step.from);
        Ok(StepEffect { step, to, capture })
    }

    /// After a `color` step from `from`, removes a piece of that color left
    /// alone on a trap. Only traps next to `from` can be affected (the
    /// destination is one of them), and no square touches two traps. Loaded
    /// positions may already hold unguarded trap pieces elsewhere; like
    /// pyrimaa, those stay until a step next to them.
    fn resolve_trap(&mut self, color: Color, from: Square) -> Option<Capture> {
        let own = self.occupied_by(color);
        let unguarded = own & TRAP_BITS & neighbors(from.bit()) & !neighbors(own);
        let sq = squares(unguarded).next()?;
        debug_assert_eq!(unguarded.count_ones(), 1, "a step can capture at most one piece");
        let piece = self.piece_at(sq).expect("trap square is occupied");
        self.remove(piece, sq);
        Some(Capture { piece, square: sq })
    }

    /// Reverses [`Position::apply_step`]. `effect` must be the most recent
    /// step applied to this position.
    pub fn undo_step(&mut self, effect: &StepEffect) {
        if let Some(c) = effect.capture {
            self.put(c.piece, c.square);
        }
        self.shift(effect.step.piece, effect.to, effect.step.from);
    }

    /// AEI short format, e.g. `[rrrrrrrr...RRRRRRRR]`: 64 characters from a8
    /// to h8, then a7 to h7, down to h1, with spaces for empty squares.
    pub fn to_short_string(&self) -> String {
        let mut s = String::with_capacity(66);
        s.push('[');
        for rank in (0..8).rev() {
            for file in 0..8 {
                let sq = Square::from_coords(file, rank).unwrap();
                s.push(self.piece_at(sq).map_or(' ', Piece::letter));
            }
        }
        s.push(']');
        s
    }

    /// Parses the AEI short format (see [`Position::to_short_string`]).
    pub fn from_short_string(side: Color, s: &str) -> Result<Position, ParseError> {
        let inner = s
            .strip_prefix('[')
            .and_then(|s| s.strip_suffix(']'))
            .ok_or_else(|| ParseError::new("short position must be wrapped in [ ]"))?;
        let chars: Vec<char> = inner.chars().collect();
        if chars.len() != 64 {
            return Err(ParseError::new(format!(
                "short position must have 64 squares, found {}",
                chars.len()
            )));
        }
        let mut pieces = Vec::new();
        for (i, &c) in chars.iter().enumerate() {
            let sq = Square::from_coords((i % 8) as u8, 7 - (i / 8) as u8).unwrap();
            match c {
                ' ' | '.' | 'x' | 'X' => {}
                _ => {
                    let p = Piece::from_letter(c)
                        .ok_or_else(|| ParseError::new(format!("invalid piece {c:?} on {sq}")))?;
                    pieces.push((p, sq));
                }
            }
        }
        Position::from_pieces(side, pieces)
    }

    /// AEI long format board diagram (without the move-number line).
    pub fn to_board_string(&self) -> String {
        let mut s = String::from(" +-----------------+\n");
        for rank in (0..8).rev() {
            s.push_str(&format!("{}|", rank + 1));
            for file in 0..8 {
                let sq = Square::from_coords(file, rank).unwrap();
                let c = match self.piece_at(sq) {
                    Some(p) => p.letter(),
                    None if sq.is_trap() => 'X',
                    None => ' ',
                };
                s.push(' ');
                s.push(c);
            }
            s.push_str(" |\n");
        }
        s.push_str(" +-----------------+\n   a b c d e f g h\n");
        s
    }
}

impl fmt::Debug for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} to move\n{}", self.side, self.to_board_string())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::types::Dir;

    /// Builds a position from a compact list like `"Ed4 rd5 Cc3"`.
    pub fn pos(side: Color, spec: &str) -> Position {
        let pieces = spec.split_whitespace().map(|t| {
            let piece = Piece::from_letter(t.chars().next().unwrap()).unwrap();
            (piece, t[1..].parse().unwrap())
        });
        Position::from_pieces(side, pieces).unwrap()
    }

    pub fn sq(s: &str) -> Square {
        s.parse().unwrap()
    }

    pub fn step(s: &str) -> Step {
        let piece = Piece::from_letter(s.chars().next().unwrap()).unwrap();
        let dir = Dir::from_letter(s.chars().nth(3).unwrap()).unwrap();
        Step::new(piece, sq(&s[1..3]), dir)
    }

    #[test]
    fn neighbors_bitboard_wraps_correctly() {
        assert_eq!(neighbors(sq("a1").bit()), sq("a2").bit() | sq("b1").bit());
        assert_eq!(neighbors(sq("h4").bit()), sq("h3").bit() | sq("h5").bit() | sq("g4").bit());
        assert_eq!(neighbors(sq("a5").bit()) & sq("h4").bit(), 0);
    }

    /// Checks the cumulative-board invariants against a plain square list.
    fn assert_consistent(p: &Position) {
        for c in Color::ALL {
            let boards = &p.at_least[c.index()];
            assert_eq!(boards[6], 0, "sentinel board must stay empty");
            for k in 1..7 {
                assert_eq!(boards[k] & !boards[k - 1], 0, "each board is a subset of the one below");
            }
        }
        assert_eq!(p.at_least[0][0] & p.at_least[1][0], 0, "a square holds one piece");
        for sq in Square::all() {
            match p.piece_at(sq) {
                Some(piece) => {
                    assert_ne!(p.bitboard(piece) & sq.bit(), 0);
                    for kind in PieceKind::ALL {
                        let expected = kind <= piece.kind;
                        assert_eq!(p.at_least(piece.color, kind) & sq.bit() != 0, expected);
                    }
                }
                None => assert_eq!(p.occupied() & sq.bit(), 0),
            }
        }
    }

    #[test]
    fn strength_boards() {
        let start = pos(Color::Gold, "Ed4 Me3 Hb5 Dg4 Ca2 Rh2 ed6 rc5 cb4 mg6");
        let mut p = start.clone();
        assert_consistent(&p);
        assert_eq!(p.stronger_than(Color::Gold, PieceKind::Horse), sq("d4").bit() | sq("e3").bit());
        assert_eq!(p.stronger_than(Color::Gold, PieceKind::Elephant), 0);
        assert_eq!(p.weaker_than(Color::Silver, PieceKind::Camel), sq("c5").bit() | sq("b4").bit());
        assert_eq!(p.weaker_than(Color::Gold, PieceKind::Rabbit), 0);
        assert_eq!(p.at_least(Color::Gold, PieceKind::Rabbit), p.occupied_by(Color::Gold));
        assert_eq!(p.bitboard(Piece::from_letter('m').unwrap()), sq("g6").bit());

        // Steps, a capture (the rabbit on c3 loses its guard) and undo all
        // keep the boards consistent.
        let mut effects = Vec::new();
        for s in ["Ed4e", "Me3w", "rc5s", "cb4s", "rc4s", "cb3w", "Ca2e"] {
            effects.push(p.apply_step(step(s)).unwrap());
            assert_consistent(&p);
        }
        assert_eq!(effects[5].capture.map(|c| c.to_string()), Some("rc3x".to_string()));
        for e in effects.iter().rev() {
            p.undo_step(e);
            assert_consistent(&p);
        }
        assert_eq!(p, start);
    }

    #[test]
    fn frozen_by_stronger_enemy() {
        let p = pos(Color::Gold, "Dd4 hd5");
        assert!(p.is_frozen(sq("d4")));
        assert!(!p.is_frozen(sq("d5")), "stronger piece is never frozen by weaker");
    }

    #[test]
    fn equal_strength_does_not_freeze() {
        let p = pos(Color::Gold, "Dd4 dd5");
        assert!(!p.is_frozen(sq("d4")));
        assert!(!p.is_frozen(sq("d5")));
    }

    #[test]
    fn friend_thaws() {
        let p = pos(Color::Gold, "Dd4 hd5 Rc4");
        assert!(!p.is_frozen(sq("d4")));
        // A weak friend works just as well as a strong one.
        let p = pos(Color::Gold, "Ed4 Rc4");
        assert!(!p.is_frozen(sq("d4")));
    }

    #[test]
    fn frozen_diagonal_does_not_count() {
        let p = pos(Color::Gold, "Dd4 he5");
        assert!(!p.is_frozen(sq("d4")));
    }

    #[test]
    fn elephant_never_frozen() {
        let p = pos(Color::Gold, "Ed4 ed5 md3 hc4 he4");
        assert!(!p.is_frozen(sq("d4")));
    }

    #[test]
    fn step_onto_trap_alone_is_captured() {
        let mut p = pos(Color::Gold, "Rc4");
        let e = p.apply_step(step("Rc4s")).unwrap();
        assert_eq!(e.capture, Some(Capture { piece: Piece::from_letter('R').unwrap(), square: sq("c3") }));
        assert_eq!(p.piece_at(sq("c3")), None);
        p.undo_step(&e);
        assert_eq!(p, pos(Color::Gold, "Rc4"));
    }

    #[test]
    fn step_onto_guarded_trap_survives() {
        let mut p = pos(Color::Gold, "Rc4 Rb3");
        let e = p.apply_step(step("Rc4s")).unwrap();
        assert_eq!(e.capture, None);
        assert!(p.piece_at(sq("c3")).is_some());
    }

    #[test]
    fn leaving_a_trap_guard_captures_friend() {
        let mut p = pos(Color::Gold, "Dc3 Rd3");
        let e = p.apply_step(step("Rd3n")).unwrap();
        assert_eq!(e.capture.map(|c| c.square), Some(sq("c3")));
        assert_eq!(p, pos(Color::Gold, "Rd4"));
        p.undo_step(&e);
        assert_eq!(p, pos(Color::Gold, "Dc3 Rd3"));
    }

    #[test]
    fn enemy_adjacent_to_trap_does_not_protect() {
        let mut p = pos(Color::Gold, "Rb3 Rd3 dc4");
        let e = p.apply_step(step("dc4s")).unwrap();
        assert!(e.capture.is_some());
    }

    #[test]
    fn unrelated_unguarded_trap_piece_is_left_alone() {
        // A loaded position can hold an unguarded piece on a trap (here Cc3).
        // A step elsewhere doesn't capture it; only a step next to the trap does.
        let mut p = pos(Color::Gold, "Cc3 Rh2 Rb4");
        assert_eq!(p.apply_step(step("Rh2n")).unwrap().capture, None);
        let e = p.apply_step(step("Rb4n")).unwrap();
        assert_eq!(e.capture, None, "b4 isn't next to c3");
        let mut p = pos(Color::Gold, "Cc3 Rd4");
        assert_eq!(p.apply_step(step("Rd4n")).unwrap().capture, None, "d4 isn't next to c3");
        let mut p = pos(Color::Gold, "Cc3 Rd3");
        assert!(p.apply_step(step("Rd3n")).unwrap().capture.is_some());
    }

    #[test]
    fn apply_step_mechanical_errors() {
        let mut p = pos(Color::Gold, "Ra1 Rb1");
        assert_eq!(p.apply_step(step("Ra1e")), Err(StepError::Occupied(sq("b1"))));
        assert_eq!(p.apply_step(step("Ra1w")), Err(StepError::OffBoard));
        assert_eq!(p.apply_step(step("Rc1n")), Err(StepError::NoPiece(sq("c1"))));
        assert!(matches!(p.apply_step(step("Ca1n")), Err(StepError::PieceMismatch { .. })));
    }

    #[test]
    fn hash_tracks_pieces_and_side() {
        let a = pos(Color::Gold, "Ed4 rd5");
        let mut b = pos(Color::Gold, "Ed4 rd6");
        assert_ne!(a.hash(), b.hash());
        let e = b.apply_step(step("rd6s")).unwrap();
        assert_eq!(a.hash(), b.hash());
        b.set_side_to_move(Color::Silver);
        assert_ne!(a.hash(), b.hash());
        assert!(a.same_board(&b));
        b.undo_step(&e);
        assert!(!a.same_board(&b));
    }

    #[test]
    fn short_string_roundtrip() {
        let s = "[rrrrrrrrhdcemcdh                                HDCMECDHRRRRRRRR]";
        let p = Position::from_short_string(Color::Gold, s).unwrap();
        assert_eq!(p.piece_at(sq("a8")), Piece::from_letter('r'));
        assert_eq!(p.piece_at(sq("d7")), Piece::from_letter('e'));
        assert_eq!(p.piece_at(sq("e2")), Piece::from_letter('E'));
        assert_eq!(p.to_short_string(), s);
        assert!(Position::from_short_string(Color::Gold, "[rr]").is_err());
    }

    #[test]
    fn board_string_matches_aei_layout() {
        let p = pos(Color::Gold, "Ed4 rh8");
        let s = p.to_board_string();
        let lines: Vec<&str> = s.lines().collect();
        assert_eq!(lines[1], "8|               r |");
        assert_eq!(lines[3], "6|     X     X     |");
        assert_eq!(lines[5], "4|       E         |");
        assert_eq!(lines[10], "   a b c d e f g h");
    }
}
