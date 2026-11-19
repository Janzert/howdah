//! The compact move encoding.
//!
//! A line of moves is a format byte ([`FORMAT`]) and then each move:
//! - a setup is 8 bytes, a nibble for each of the mover's 16 home squares
//!   in square order (a1, b1, …, h2 for gold; a7, …, h8 for silver): 0 for
//!   an empty square, else the piece kind's index plus 1;
//! - a turn is a byte with its number of steps (0 for a pass), then a byte
//!   per step: the from-square's index times 4 plus the direction
//!   (N, S, E, W). The piece and any capture follow from the position.
//!
//! So a turn takes at most 5 bytes. Whether a move is a setup follows from
//! its ply, so decoding needs the position it starts from.
//!
//! Move times are a separate blob ([`encode_times`]): a format byte, then
//! per ply an unsigned LEB128 of the milliseconds plus 1, with 0 for a
//! move whose time isn't known.

use howdah_arimaa::{
    Color, Dir, GameError, GameTree, Move, NodeId, Piece, PieceKind, Placement, Square, Step,
};

/// The current line format.
pub const FORMAT: u8 = 1;
pub const TIMES_FORMAT: u8 = 1;

#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    #[error("unknown move format {0}")]
    Format(u8),
    #[error("the moves end in the middle of a move")]
    Truncated,
    #[error("a setup piece off the mover's home squares")]
    SetupSquare,
    #[error("no piece on {0} to step")]
    NoPiece(Square),
    #[error("bad setup nibble {0}")]
    SetupNibble(u8),
    #[error("a turn of {0} steps")]
    StepCount(u8),
    #[error(transparent)]
    Game(#[from] GameError),
    #[error("step {step}: {source}")]
    Step { step: String, source: howdah_arimaa::StepError },
    #[error("turn: {0}")]
    Commit(#[from] howdah_arimaa::CommitError),
}

fn home_squares(color: Color) -> impl Iterator<Item = Square> {
    let [a, b] = color.home_ranks();
    let (lo, hi) = (a.min(b), a.max(b));
    (lo * 8..(hi + 1) * 8).map(|i| Square::new(i).expect("on the board"))
}

/// Appends one move.
pub fn encode_move(out: &mut Vec<u8>, mv: &Move) -> Result<(), CodecError> {
    match mv {
        Move::Setup(placements) => {
            let Some(color) = placements.first().map(|p| p.piece.color) else {
                out.extend_from_slice(&[0; 8]);
                return Ok(());
            };
            let mut nibbles = [0u8; 16];
            let squares: Vec<Square> = home_squares(color).collect();
            for p in placements {
                let i = squares.iter().position(|&s| s == p.square).ok_or(CodecError::SetupSquare)?;
                if p.piece.color != color {
                    return Err(CodecError::SetupSquare);
                }
                nibbles[i] = p.piece.kind.index() as u8 + 1;
            }
            for pair in nibbles.chunks(2) {
                out.push(pair[0] | pair[1] << 4);
            }
        }
        Move::Steps(effects) => {
            out.push(effects.len() as u8);
            for e in effects {
                let dir = Dir::ALL.iter().position(|&d| d == e.step.dir).expect("a direction") as u8;
                out.push(e.step.from.index() << 2 | dir);
            }
        }
        Move::Pass => out.push(0),
    }
    Ok(())
}

/// Encodes a line of moves.
pub fn encode_line<'a>(moves: impl IntoIterator<Item = &'a Move>) -> Result<Vec<u8>, CodecError> {
    let mut out = vec![FORMAT];
    for mv in moves {
        encode_move(&mut out, mv)?;
    }
    Ok(out)
}

/// Reads moves from a byte slice.
pub struct Reader<'a> {
    bytes: &'a [u8],
}

impl<'a> Reader<'a> {
    /// A reader over a line's blob, after checking its format byte.
    pub fn line(blob: &'a [u8]) -> Result<Reader<'a>, CodecError> {
        match blob.split_first() {
            Some((&FORMAT, rest)) => Ok(Reader { bytes: rest }),
            Some((&f, _)) => Err(CodecError::Format(f)),
            None => Err(CodecError::Truncated),
        }
    }

    /// A reader over a single move with no format byte (a variation node).
    pub fn single(bytes: &'a [u8]) -> Reader<'a> {
        Reader { bytes }
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// The next move's bytes, without reading it: a setup when `setup`
    /// (the tree's [`GameTree::setup_due`]), else a turn.
    pub fn next_bytes(&mut self, setup: bool) -> Result<&'a [u8], CodecError> {
        let n = if setup { 8 } else { 1 + *self.bytes.first().ok_or(CodecError::Truncated)? as usize };
        self.take(n)
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], CodecError> {
        if self.bytes.len() < n {
            return Err(CodecError::Truncated);
        }
        let (head, rest) = self.bytes.split_at(n);
        self.bytes = rest;
        Ok(head)
    }

    /// Reads the next move and adds it to `tree` after `parent`, checking it
    /// with the rules. Returns the new node.
    pub fn add_next(&mut self, tree: &mut GameTree, parent: NodeId) -> Result<NodeId, CodecError> {
        if tree.setup_due(parent) {
            let color = tree[parent].position().side_to_move();
            let bytes = self.take(8)?;
            let mut placements = Vec::with_capacity(16);
            for (i, sq) in home_squares(color).enumerate() {
                let nibble = bytes[i / 2] >> (4 * (i % 2)) & 0xF;
                if nibble == 0 {
                    continue;
                }
                let kind = *PieceKind::ALL.get(nibble as usize - 1).ok_or(CodecError::SetupNibble(nibble))?;
                placements.push(Placement { piece: Piece::new(color, kind), square: sq });
            }
            return Ok(tree.add_setup(parent, placements)?);
        }
        let count = self.take(1)?[0];
        if count == 0 {
            return Ok(tree.add_pass(parent)?);
        }
        if count as usize > howdah_arimaa::MAX_STEPS {
            return Err(CodecError::StepCount(count));
        }
        let mut tb = tree.begin_turn(parent)?;
        for &b in self.take(count as usize)? {
            let from = Square::new(b >> 2).expect("six bits");
            let dir = Dir::ALL[(b & 3) as usize];
            let piece = tb.position().piece_at(from).ok_or(CodecError::NoPiece(from))?;
            let step = Step::new(piece, from, dir);
            tb.try_step(step).map_err(|source| CodecError::Step { step: step.to_string(), source })?;
        }
        Ok(tree.add_turn(parent, tb.finish()?)?)
    }
}

/// A line's moves, each as its bytes, without playing them. `start_ply`
/// is the absolute ply of the line's first position (0 from the setups),
/// which says which moves are setups.
pub fn split_line(blob: &[u8], start_ply: usize) -> Result<Vec<&[u8]>, CodecError> {
    let mut r = Reader::line(blob)?;
    let mut out = Vec::new();
    while !r.is_empty() {
        out.push(r.next_bytes(start_ply + out.len() < 2)?);
    }
    Ok(out)
}

/// Encodes move times in milliseconds, `None` for unknown ones.
pub fn encode_times(times: &[Option<u64>]) -> Vec<u8> {
    let mut out = vec![TIMES_FORMAT];
    for t in times {
        let mut v = t.map_or(0, |ms| ms + 1);
        loop {
            let b = (v & 0x7F) as u8;
            v >>= 7;
            if v == 0 {
                out.push(b);
                break;
            }
            out.push(b | 0x80);
        }
    }
    out
}

pub fn decode_times(blob: &[u8]) -> Result<Vec<Option<u64>>, CodecError> {
    let rest = match blob.split_first() {
        Some((&TIMES_FORMAT, rest)) => rest,
        Some((&f, _)) => return Err(CodecError::Format(f)),
        None => return Err(CodecError::Truncated),
    };
    let mut out = Vec::new();
    let (mut v, mut shift) = (0u64, 0);
    for &b in rest {
        v |= u64::from(b & 0x7F) << shift;
        if b & 0x80 == 0 {
            out.push(v.checked_sub(1));
            (v, shift) = (0, 0);
        } else {
            shift += 7;
            if shift > 63 {
                return Err(CodecError::Truncated);
            }
        }
    }
    if shift != 0 {
        return Err(CodecError::Truncated);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use howdah_arimaa::GameRecord;

    const GAME: &str = "1g Ra1 Rb1 Rc1 Rd1 Re1 Rf1 Rg1 Rh1 Da2 Hb2 Cc2 Ed2 Me2 Hf2 Cg2 Dh2\n\
        1s ch7 hg7 df7 me7 ed7 dc7 hb7 ca7 rh8 rg8 rf8 re8 rd8 rc8 rb8 ra8\n\
        2g Ed2n Ed3n Me2n Me3n\n2s ca7s ra8s ca6s ra7s\n3g Da2n Da3n Da4s ca5s\n\
        3s ra6s ra5e rb5s rb4s\n4g Ed4n Ed5n Ed6e Cg2n\n";

    #[test]
    fn line_round_trips() {
        let record = GameRecord::parse(GAME).unwrap();
        let tree = &record.tree;
        let main = tree.main_line();
        let blob = encode_line(main[1..].iter().map(|&n| tree[n].mv().unwrap())).unwrap();
        // Two setups of 8 bytes, then 5 turns of 1 + 4 bytes.
        assert_eq!(blob.len(), 1 + 16 + 5 * 5);
        let mut back = GameTree::new();
        let mut at = back.root();
        let mut r = Reader::line(&blob).unwrap();
        while !r.is_empty() {
            at = r.add_next(&mut back, at).unwrap();
        }
        for (&a, &b) in main.iter().zip(&back.main_line()) {
            assert_eq!(tree[a].position(), back[b].position());
        }
        assert_eq!(back.main_line().len(), main.len());
        // Turns come back whole (pieces and captures from the position);
        // setups only by position, since their order is canonical.
        for (&a, &b) in main[3..].iter().zip(&back.main_line()[3..]) {
            assert_eq!(tree[a].mv(), back[b].mv());
        }
    }

    #[test]
    fn times_round_trip() {
        let times = [Some(0), Some(70_000), None, Some(1), Some(u64::MAX - 1)];
        assert_eq!(decode_times(&encode_times(&times)).unwrap(), times);
    }

    #[test]
    fn unknown_format_refused() {
        assert!(matches!(Reader::line(&[9]), Err(CodecError::Format(9))));
    }
}
