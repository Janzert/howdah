//! Start positions (`[positions]` in a spec): games that begin from moves
//! already played instead of from the setups.
//!
//! The file has one position per line, as the moves of a game from the
//! start: `1g Ra1 Rb1 … 1s ra8 … 2g Ee2n Ee3n …`, each move led by its
//! number. Blank lines and anything after a `#` are ignored. Each line is
//! checked as a game: its moves must be legal, and it must not have ended.
//! Engines are sent the moves (`makemove`, not `setposition`), so they
//! know the history.
//!
//! With `sides = "both"` (the default), pair `p` plays position `p` from
//! both sides, cycling through the file, as an opening book does. With
//! `sides = "to_move"`, engine 1 always plays the side to move: game 1 of
//! pair `p` takes the `p`-th position with gold to move (engine 1 is gold
//! in game 1) and game 2 the `p`-th with silver to move, each list
//! cycling on its own, so the file needs positions of both kinds.

use std::path::{Path, PathBuf};

use howdah_arimaa::notation::parse_move_number;
use howdah_arimaa::{Color, Game};
use serde::{Deserialize, Serialize};

/// `[positions]` in a spec.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PositionsSpec {
    /// The positions file. In a test's directory, its copy there,
    /// relative to the directory.
    pub file: PathBuf,
    #[serde(default, skip_serializing_if = "PositionSides::is_default")]
    pub sides: PositionSides,
}

/// Which engine plays which side of a start position.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PositionSides {
    /// Each position from both sides, one game each.
    #[default]
    Both,
    /// Engine 1 plays the side to move in every game.
    ToMove,
}

impl PositionSides {
    fn is_default(&self) -> bool {
        *self == PositionSides::Both
    }
}

/// One start position: its line in the file (from 1) and its moves.
#[derive(Clone, Debug)]
pub struct StartPosition {
    pub line: u32,
    pub game: Game,
}

/// A positions file, read and checked.
#[derive(Clone, Debug)]
pub struct Positions {
    all: Vec<StartPosition>,
    sides: PositionSides,
    /// Indexes into `all` with gold, and with silver, to move.
    to_move: [Vec<usize>; 2],
}

impl Positions {
    pub const FILE: &'static str = "positions.txt";

    pub fn parse(text: &str, sides: PositionSides) -> Result<Positions, String> {
        let mut all = Vec::new();
        for (i, raw) in text.lines().enumerate() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let n = i as u32 + 1;
            let game = parse_line(line).map_err(|e| format!("positions line {n}: {e}"))?;
            all.push(StartPosition { line: n, game });
        }
        if all.is_empty() {
            return Err("the positions file has no positions".into());
        }
        let mut to_move: [Vec<usize>; 2] = Default::default();
        for (i, p) in all.iter().enumerate() {
            to_move[p.game.current_position().side_to_move().index()].push(i);
        }
        if sides == PositionSides::ToMove && to_move.iter().any(Vec::is_empty) {
            let missing = if to_move[0].is_empty() { "gold" } else { "silver" };
            return Err(format!(
                "sides = \"to_move\" needs positions with each side to move, and none has {missing} to move"
            ));
        }
        Ok(Positions { all, sides, to_move })
    }

    pub fn load(path: &Path, sides: PositionSides) -> Result<Positions, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Positions::parse(&text, sides)
    }

    pub fn len(&self) -> usize {
        self.all.len()
    }

    pub fn is_empty(&self) -> bool {
        self.all.is_empty()
    }

    /// The position game `game` (1 or 2) of pair `pair` (from 1) starts from.
    pub fn for_game(&self, pair: u32, game: u8) -> &StartPosition {
        let k = pair.saturating_sub(1) as usize;
        match self.sides {
            PositionSides::Both => &self.all[k % self.all.len()],
            PositionSides::ToMove => {
                let list = &self.to_move[usize::from(game == 2)];
                &self.all[list[k % list.len()]]
            }
        }
    }
}

/// A line's moves as a game: split at the move numbers, then read as a
/// record (one move a line), which checks them.
fn parse_line(line: &str) -> Result<Game, String> {
    let mut record = String::new();
    for tok in line.split_whitespace() {
        if parse_move_number(tok).is_ok() {
            if !record.is_empty() {
                record.push('\n');
            }
        } else if record.is_empty() {
            return Err(format!("expected a move number such as 1g, not {tok:?}"));
        } else {
            record.push(' ');
        }
        record.push_str(tok);
    }
    let game = Game::parse(&record).map_err(|e| e.to_string())?;
    if game.start_position().is_some() {
        return Err("a position must be the moves from the start".into());
    }
    if let Some(r) = game.result() {
        return Err(format!(
            "the game is over ({} won)",
            if r.winner == Color::Gold { "gold" } else { "silver" }
        ));
    }
    Ok(game)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SETUPS: &str = "1g Ra1 Rb1 Rc1 Rd1 Re1 Rf1 Rg1 Rh1 Ha2 Db2 Cc2 Md2 Ee2 Cf2 Dg2 Hh2 \
                          1s ra8 rb8 rc8 rd8 re8 rf8 rg8 rh8 ha7 db7 cc7 ed7 me7 cf7 dg7 hh7";

    #[test]
    fn reads_positions_and_skips_comments() {
        let text = format!("# won positions\n\n{SETUPS} 2g Ee2n Ee3n  # line 3\n{SETUPS}\n");
        let p = Positions::parse(&text, PositionSides::Both).unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!(p.for_game(1, 1).line, 3);
        assert_eq!(p.for_game(1, 1).game.ply_count(), 3);
        assert_eq!(p.for_game(1, 2).line, 3);
        assert_eq!(p.for_game(2, 1).line, 4);
        assert_eq!(p.for_game(3, 2).line, 3, "cycles");
    }

    #[test]
    fn to_move_gives_engine_one_the_side_to_move() {
        // Lines 1 and 3: gold to move; line 2: silver.
        let text = format!("{SETUPS}\n{SETUPS} 2g Ee2n\n{SETUPS} 2g Ee2n 2s ed7s\n");
        let p = Positions::parse(&text, PositionSides::ToMove).unwrap();
        let side = |pair, game| p.for_game(pair, game).game.current_position().side_to_move();
        assert_eq!([p.for_game(1, 1).line, p.for_game(2, 1).line, p.for_game(3, 1).line], [1, 3, 1]);
        assert_eq!([p.for_game(1, 2).line, p.for_game(2, 2).line], [2, 2]);
        for pair in 1..4 {
            assert_eq!(side(pair, 1), Color::Gold);
            assert_eq!(side(pair, 2), Color::Silver);
        }
        let one_side = format!("{SETUPS}\n");
        let err = Positions::parse(&one_side, PositionSides::ToMove).unwrap_err();
        assert!(err.contains("silver to move"), "{err}");
    }

    #[test]
    fn refuses_bad_lines() {
        let bad = |text: &str| Positions::parse(text, PositionSides::Both).unwrap_err();
        assert!(bad("").contains("no positions"));
        assert!(bad("Ra1 Rb1").contains("line 1"));
        let illegal = format!("x\n{SETUPS} 2g Ee2s");
        assert!(bad(&illegal).contains("line 1"), "the first bad line is named");
        assert!(bad(&format!("{SETUPS} 2g Ee2s")).contains("line 1"));
    }
}
