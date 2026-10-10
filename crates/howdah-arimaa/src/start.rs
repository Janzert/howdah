//! Set positions: games that start after the setups, from a position given
//! in the short or long format or made in a position editor.
//!
//! A set position stands at the start of a turn, after both setups, so it
//! has a side to move and a move number (`2g` unless the text says
//! otherwise) but no steps taken and no history: repetition counts from
//! it.

use std::fmt;

use crate::error::ParseError;
use crate::outcome::is_immobilized;
use crate::position::Position;
use crate::types::{Color, Piece, PieceKind, Square, TRAPS};

/// A position to start a game from, standing before move `move_number` of
/// its side to move.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartPosition {
    pub position: Position,
    pub move_number: u32,
}

/// Why a position can't start a game. The checks are 4steps' for its
/// puzzles, plus a side to move that can't move.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PositionProblem {
    /// More of a piece than a side starts with.
    TooMany { piece: Piece, count: u32 },
    /// A piece on a trap with no friendly piece next to it, which the trap
    /// would already have taken.
    UnsupportedOnTrap { piece: Piece, square: Square },
    /// A rabbit on the row it's heading for: the game is already won.
    RabbitOnGoal { piece: Piece, square: Square },
    /// A side with no rabbits loses by elimination.
    NoRabbits(Color),
    /// The side to move has no legal step.
    NoMoves(Color),
}

impl PositionProblem {
    /// The squares the problem is about, to point them out on a board.
    pub fn squares(&self, position: &Position) -> Vec<Square> {
        match *self {
            PositionProblem::TooMany { piece, .. } => {
                position.pieces().filter(|&(_, p)| p == piece).map(|(sq, _)| sq).collect()
            }
            PositionProblem::UnsupportedOnTrap { square, .. }
            | PositionProblem::RabbitOnGoal { square, .. } => {
                vec![square]
            }
            PositionProblem::NoRabbits(_) | PositionProblem::NoMoves(_) => Vec::new(),
        }
    }
}

fn color_name(c: Color) -> &'static str {
    match c {
        Color::Gold => "gold",
        Color::Silver => "silver",
    }
}

fn kind_name(k: PieceKind, plural: bool) -> &'static str {
    match (k, plural) {
        (PieceKind::Rabbit, false) => "rabbit",
        (PieceKind::Cat, false) => "cat",
        (PieceKind::Dog, false) => "dog",
        (PieceKind::Horse, false) => "horse",
        (PieceKind::Camel, false) => "camel",
        (PieceKind::Elephant, false) => "elephant",
        (PieceKind::Rabbit, true) => "rabbits",
        (PieceKind::Cat, true) => "cats",
        (PieceKind::Dog, true) => "dogs",
        (PieceKind::Horse, true) => "horses",
        (PieceKind::Camel, true) => "camels",
        (PieceKind::Elephant, true) => "elephants",
    }
}

impl fmt::Display for PositionProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            PositionProblem::TooMany { piece, count } => {
                let max = piece.kind.initial_count();
                let plural = kind_name(piece.kind, true);
                write!(f, "{count} {} {plural}; a side has at most {max}", color_name(piece.color))
            }
            PositionProblem::UnsupportedOnTrap { piece, square } => {
                write!(
                    f,
                    "{} {} alone on trap {square}",
                    capitalized(piece.color),
                    kind_name(piece.kind, false)
                )
            }
            PositionProblem::RabbitOnGoal { piece, square } => {
                write!(f, "{} rabbit on its goal row ({square})", capitalized(piece.color))
            }
            PositionProblem::NoRabbits(c) => write!(f, "{} has no rabbits", capitalized(c)),
            PositionProblem::NoMoves(c) => write!(f, "{}, to move, has no legal moves", capitalized(c)),
        }
    }
}

fn capitalized(c: Color) -> &'static str {
    match c {
        Color::Gold => "Gold",
        Color::Silver => "Silver",
    }
}

/// Everything that keeps `position` from starting a game, in board order
/// within each kind of problem; empty if it can.
pub fn check_start_position(position: &Position) -> Vec<PositionProblem> {
    let mut problems = Vec::new();
    for color in Color::ALL {
        for kind in PieceKind::ALL.into_iter().rev() {
            let piece = Piece::new(color, kind);
            let count = position.count(piece);
            if count > kind.initial_count() {
                problems.push(PositionProblem::TooMany { piece, count });
            }
        }
    }
    for trap in TRAPS {
        if let Some(piece) = position.piece_at(trap)
            && !trap.neighbors().any(|n| position.piece_at(n).is_some_and(|p| p.color == piece.color))
        {
            problems.push(PositionProblem::UnsupportedOnTrap { piece, square: trap });
        }
    }
    for (square, piece) in position.pieces() {
        if piece.kind == PieceKind::Rabbit && square.rank() == piece.color.goal_rank() {
            problems.push(PositionProblem::RabbitOnGoal { piece, square });
        }
    }
    for color in Color::ALL {
        if position.count(Piece::new(color, PieceKind::Rabbit)) == 0 {
            problems.push(PositionProblem::NoRabbits(color));
        }
    }
    if problems.is_empty() && is_immobilized(position) {
        problems.push(PositionProblem::NoMoves(position.side_to_move()));
    }
    problems
}

impl StartPosition {
    /// Gold to move at `2g`, the first turn after the setups.
    pub fn new(position: Position) -> StartPosition {
        StartPosition { position, move_number: 2 }
    }

    /// The move label of the first turn, such as `2g`.
    pub fn label(&self) -> String {
        format!("{}{}", self.move_number, self.position.side_to_move().letter())
    }

    /// The short format with the side to move, as AEI's `setposition` and
    /// the record's `Position` tag take it: `g [rrrrrrrr...]`.
    pub fn to_short_string(&self) -> String {
        format!("{} {}", self.position.side_to_move().letter(), self.position.to_short_string())
    }

    /// The long format: the move label, then the board diagram.
    pub fn to_long_string(&self) -> String {
        format!("{}\n{}", self.label(), self.position.to_board_string())
    }

    /// Reads a position in the short format (`[` + 64 squares + `]`,
    /// optionally after a side letter or a move label: `s [...]`,
    /// `12s [...]`) or the long format (the board diagram, optionally after
    /// its move label line). Without a side, gold is to move; without a
    /// move number, it's move 2. `w` and `b` are read as gold and silver.
    pub fn parse(text: &str) -> Result<StartPosition, ParseError> {
        let text = text.trim();
        if text.is_empty() {
            return Err(ParseError::new("no position given"));
        }
        let (label, rest) = match text.find(['[', '\n', '|', '+']) {
            Some(i) => (text[..i].trim(), &text[i..]),
            None => (text, ""),
        };
        let (move_number, side) = parse_label(label)?;
        let rest = rest.trim();
        let position = if rest.starts_with('[') {
            let end =
                rest.find(']').ok_or_else(|| ParseError::new("short position must be wrapped in [ ]"))?;
            if !rest[end + 1..].trim().is_empty() {
                return Err(ParseError::new("unexpected text after the position"));
            }
            Position::from_short_string(side, &rest[..=end])?
        } else {
            parse_board(side, rest)?
        };
        Ok(StartPosition { position, move_number: move_number.unwrap_or(2) })
    }
}

impl StartPosition {
    /// Edits the position from typed words, as 4steps's custom setup
    /// does: a placement (`Ra1`) puts a piece on a square, replacing what
    /// was there; a piece and square with a direction moves it (`Ra1n`) or
    /// with `x` takes it off (`Ra1x`); `g` or `s` sets the side to move.
    /// Words are applied in order; on an error nothing changes.
    pub fn apply_edits(&mut self, text: &str) -> Result<(), ParseError> {
        let mut edited = self.position.clone();
        for word in text.split_whitespace() {
            let bad = || ParseError::new(format!("can't read {word:?}: expected Ra1, Ra1n, Ra1x, g or s"));
            if let Some(side) = word.chars().next().filter(|_| word.len() == 1).and_then(Color::from_letter) {
                edited.set_side_to_move(side);
                continue;
            }
            let mut chars = word.chars();
            let piece = chars.next().and_then(Piece::from_letter).ok_or_else(bad)?;
            let square: Square = word.get(1..3).ok_or_else(bad)?.parse().map_err(|_| bad())?;
            match word.get(3..) {
                Some("") => edited.set(square, Some(piece)),
                Some(action) if action.len() == 1 => {
                    if edited.piece_at(square) != Some(piece) {
                        return Err(ParseError::new(format!("{word}: no {piece} on {square}")));
                    }
                    edited.set(square, None);
                    if action != "x" {
                        let dir =
                            action.chars().next().and_then(crate::types::Dir::from_letter).ok_or_else(bad)?;
                        let to = square
                            .neighbor(dir)
                            .ok_or_else(|| ParseError::new(format!("{word}: off the board")))?;
                        edited.set(to, Some(piece));
                    }
                }
                _ => return Err(bad()),
            }
        }
        self.position = edited;
        Ok(())
    }
}

/// A move label or a side letter: `12s`, `g`, or nothing.
fn parse_label(label: &str) -> Result<(Option<u32>, Color), ParseError> {
    if label.is_empty() {
        return Ok((None, Color::Gold));
    }
    let bad = || ParseError::new(format!("expected a side (g or s) or a move label (12s), found {label:?}"));
    let digits = label.len() - label.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    let (number, letter) = label.split_at(digits);
    let mut chars = letter.chars();
    let side = match (chars.next(), chars.next()) {
        (Some(c), None) => Color::from_letter(c).ok_or_else(bad)?,
        _ => return Err(bad()),
    };
    if number.is_empty() {
        return Ok((None, side));
    }
    let n: u32 = number.parse().map_err(|_| bad())?;
    if n < 2 {
        return Err(ParseError::new("a set position comes after the setups (move 2 or later)"));
    }
    Ok((Some(n), side))
}

/// The long format's board: eight rows like `8| r r . . X . r r |`, from
/// rank 8 down (in any order, by their rank digits), with a piece letter or
/// a blank, `.`, `x` or `X` for an empty square in every other column after
/// the bar.
fn parse_board(side: Color, text: &str) -> Result<Position, ParseError> {
    let mut rows: [Option<Vec<Option<Piece>>>; 8] = Default::default();
    for line in text.lines() {
        let line = line.trim_end();
        let Some(bar) = line.find('|') else { continue };
        let Some(rank) = line[..bar].trim().parse::<u8>().ok().filter(|r| (1..=8).contains(r)) else {
            continue;
        };
        let cells: Vec<char> = line[bar + 1..].chars().collect();
        let mut row = Vec::with_capacity(8);
        for file in 0..8 {
            let c = cells.get(2 * file + 1).copied().unwrap_or(' ');
            row.push(match c {
                ' ' | '.' | 'x' | 'X' => None,
                _ => Some(Piece::from_letter(c).ok_or_else(|| {
                    ParseError::new(format!("invalid piece {c:?} on {}{rank}", (b'a' + file as u8) as char))
                })?),
            });
        }
        if rows[rank as usize - 1].replace(row).is_some() {
            return Err(ParseError::new(format!("row {rank} appears twice")));
        }
    }
    if let Some(missing) = rows.iter().rposition(Option::is_none) {
        return Err(ParseError::new(format!(
            "expected a short position ([...]) or a board diagram; row {} is missing",
            missing + 1
        )));
    }
    let pieces = rows.iter().enumerate().flat_map(|(rank, row)| {
        row.as_ref().expect("checked").iter().enumerate().filter_map(move |(file, p)| {
            p.map(|p| (p, Square::from_coords(file as u8, rank as u8).expect("on the board")))
        })
    });
    Position::from_pieces(side, pieces)
}

/// The two default setups, gold to move: the editor's starting position.
pub fn default_start() -> Position {
    use crate::setup::{apply_setup, default_setup};
    let gold = apply_setup(&Position::empty(Color::Gold), &default_setup(Color::Gold)).expect("valid");
    apply_setup(&gold, &default_setup(Color::Silver)).expect("valid")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::position::tests::pos;

    fn problems(side: Color, spec: &str) -> Vec<String> {
        check_start_position(&pos(side, spec)).iter().map(ToString::to_string).collect()
    }

    #[test]
    fn the_default_start_is_fine() {
        assert!(check_start_position(&default_start()).is_empty());
        assert_eq!(default_start().side_to_move(), Color::Gold);
    }

    #[test]
    fn finds_what_4steps_refuses() {
        assert_eq!(problems(Color::Gold, "Ra2 rh7 Dc3"), ["Gold dog alone on trap c3"]);
        assert!(problems(Color::Gold, "Ra2 rh7 Dc3 Cc4").is_empty(), "a friend beside it holds it");
        assert_eq!(
            problems(Color::Gold, "Ra2 rh7 Dc3 dc4"),
            ["Gold dog alone on trap c3"],
            "an enemy beside it doesn't"
        );
        assert_eq!(problems(Color::Gold, "Ra8 rh7"), ["Gold rabbit on its goal row (a8)"]);
        assert!(problems(Color::Gold, "Ra1 rh8").is_empty(), "rabbits on their own back rows are fine");
        assert_eq!(problems(Color::Silver, "Ra2 rh1"), ["Silver rabbit on its goal row (h1)"]);
        assert_eq!(problems(Color::Gold, "Ra2 Ec2 Ed2 rh7"), ["2 gold elephants; a side has at most 1"]);
        assert_eq!(problems(Color::Gold, "Ra2 Da3 Db3 Dd3 rh7"), ["3 gold dogs; a side has at most 2"]);
        assert_eq!(problems(Color::Gold, "Ea2 rh7"), ["Gold has no rabbits"]);
        assert_eq!(problems(Color::Gold, ""), ["Gold has no rabbits", "Silver has no rabbits"]);
    }

    #[test]
    fn the_side_to_move_must_have_a_move() {
        // Gold's rabbit on a1 is frozen by silver cats, with no friend beside it.
        let spec = "Ra1 ca2 cb1 rh7";
        assert_eq!(problems(Color::Gold, spec), ["Gold, to move, has no legal moves"]);
        assert!(problems(Color::Silver, spec).is_empty());
    }

    #[test]
    fn problems_point_at_squares() {
        let p = pos(Color::Gold, "Ra2 Da3 Db3 Dd3 rh7");
        let found = check_start_position(&p);
        let mut squares = found[0].squares(&p);
        squares.sort();
        assert_eq!(squares.iter().map(ToString::to_string).collect::<Vec<_>>(), ["a3", "b3", "d3"]);
    }

    #[test]
    fn reads_the_short_format() {
        let p = default_start();
        let short = p.to_short_string();
        let s = StartPosition::parse(&short).unwrap();
        assert_eq!((s.position.clone(), s.move_number), (p.clone(), 2));
        let s = StartPosition::parse(&format!("s {short}")).unwrap();
        assert_eq!(s.position.side_to_move(), Color::Silver);
        assert_eq!(s.to_short_string(), format!("s {short}"));
        let s = StartPosition::parse(&format!("  12b {short} ")).unwrap();
        assert_eq!((s.move_number, s.position.side_to_move(), s.label()), (12, Color::Silver, "12s".into()));
        assert!(StartPosition::parse(&format!("1g {short}")).is_err(), "move 1 is the setups");
        assert!(StartPosition::parse(&format!("q {short}")).is_err());
        assert!(StartPosition::parse("[rr]").is_err());
        assert!(StartPosition::parse("").is_err());
    }

    #[test]
    fn edits_from_typed_words() {
        let mut s = StartPosition::new(pos(Color::Gold, "Ra2 rh7"));
        s.apply_edits("Ed4 ed5  Ra2n Hb2 Hb2x rh7w s").unwrap();
        assert_eq!(s.position, pos(Color::Silver, "Ra3 rg7 Ed4 ed5"));
        s.apply_edits("Md4").unwrap();
        assert_eq!(s.position.piece_at("d4".parse().unwrap()).map(|p| p.letter()), Some('M'), "replaces");
        let before = s.clone();
        for bad in ["Ra1x", "Zd4", "Md4q", "Md9", "Ma3n", "rg7n Ra3w Ra3w"] {
            assert!(s.apply_edits(bad).is_err(), "{bad}");
            assert_eq!(s, before, "{bad} changed nothing");
        }
    }

    #[test]
    fn reads_the_long_format() {
        let s = StartPosition { position: pos(Color::Silver, "Ra2 Dc3 Cc4 rh7 eh8"), move_number: 31 };
        let long = s.to_long_string();
        assert!(long.starts_with("31s\n +----"), "{long}");
        assert_eq!(StartPosition::parse(&long).unwrap(), s);
        // Without a label, gold at move 2; dots for blanks are fine too.
        let board = long.lines().skip(1).collect::<Vec<_>>().join("\n").replace("   ", " . ");
        let read = StartPosition::parse(&board).unwrap();
        assert_eq!(read.move_number, 2);
        assert!(read.position.same_board(&s.position));
        let short_of_rows = long.lines().filter(|l| !l.starts_with("4|")).collect::<Vec<_>>().join("\n");
        let err = StartPosition::parse(&short_of_rows).unwrap_err();
        assert!(err.to_string().contains("row 4 is missing"), "{err}");
    }
}
