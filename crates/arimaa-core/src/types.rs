//! Basic value types: colors, pieces, squares and directions.

use std::fmt;
use std::str::FromStr;

use crate::error::ParseError;

/// Derives shared by every serializable core data type.
macro_rules! data_type {
    ($item:item) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
        $item
    };
}
pub(crate) use data_type;

data_type! {
    #[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
    pub enum Color {
        Gold,
        Silver,
    }
}

impl Color {
    pub const ALL: [Color; 2] = [Color::Gold, Color::Silver];

    pub fn opponent(self) -> Color {
        match self {
            Color::Gold => Color::Silver,
            Color::Silver => Color::Gold,
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }

    /// Letter used in move numbers: `g` or `s`.
    pub fn letter(self) -> char {
        match self {
            Color::Gold => 'g',
            Color::Silver => 's',
        }
    }

    /// Parses a side letter. Accepts the old `w`/`b` letters as well as `g`/`s`.
    pub fn from_letter(c: char) -> Option<Color> {
        match c {
            'g' | 'w' | 'G' | 'W' => Some(Color::Gold),
            's' | 'b' | 'S' | 'B' => Some(Color::Silver),
            _ => None,
        }
    }

    /// The direction this side's rabbits may not move.
    pub fn backward(self) -> Dir {
        match self {
            Color::Gold => Dir::S,
            Color::Silver => Dir::N,
        }
    }

    /// Rank (0-based) a rabbit of this color must reach to win.
    pub fn goal_rank(self) -> u8 {
        match self {
            Color::Gold => 7,
            Color::Silver => 0,
        }
    }

    /// The two ranks (0-based) this side sets up on.
    pub fn home_ranks(self) -> [u8; 2] {
        match self {
            Color::Gold => [0, 1],
            Color::Silver => [6, 7],
        }
    }
}

data_type! {
    /// Piece kinds, ordered weakest to strongest so `Ord` compares strength.
    #[derive(PartialOrd, Ord)]
    #[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
    pub enum PieceKind {
        Rabbit,
        Cat,
        Dog,
        Horse,
        Camel,
        Elephant,
    }
}

impl PieceKind {
    pub const ALL: [PieceKind; 6] = [
        PieceKind::Rabbit,
        PieceKind::Cat,
        PieceKind::Dog,
        PieceKind::Horse,
        PieceKind::Camel,
        PieceKind::Elephant,
    ];

    pub fn index(self) -> usize {
        self as usize
    }

    /// Uppercase letter: R C D H M E.
    pub fn letter(self) -> char {
        match self {
            PieceKind::Rabbit => 'R',
            PieceKind::Cat => 'C',
            PieceKind::Dog => 'D',
            PieceKind::Horse => 'H',
            PieceKind::Camel => 'M',
            PieceKind::Elephant => 'E',
        }
    }

    /// Number of pieces of this kind each side starts with.
    pub fn initial_count(self) -> u32 {
        match self {
            PieceKind::Rabbit => 8,
            PieceKind::Cat | PieceKind::Dog | PieceKind::Horse => 2,
            PieceKind::Camel | PieceKind::Elephant => 1,
        }
    }
}

data_type! {
    pub struct Piece {
        pub color: Color,
        pub kind: PieceKind,
    }
}

impl Piece {
    pub const fn new(color: Color, kind: PieceKind) -> Piece {
        Piece { color, kind }
    }

    /// Standard letter: uppercase for gold, lowercase for silver.
    pub fn letter(self) -> char {
        let c = self.kind.letter();
        match self.color {
            Color::Gold => c,
            Color::Silver => c.to_ascii_lowercase(),
        }
    }

    pub fn from_letter(c: char) -> Option<Piece> {
        let kind = match c.to_ascii_uppercase() {
            'R' => PieceKind::Rabbit,
            'C' => PieceKind::Cat,
            'D' => PieceKind::Dog,
            'H' => PieceKind::Horse,
            'M' => PieceKind::Camel,
            'E' => PieceKind::Elephant,
            _ => return None,
        };
        let color = if c.is_ascii_uppercase() { Color::Gold } else { Color::Silver };
        Some(Piece { color, kind })
    }

    /// True if `self` is stronger than `other` (colors are ignored).
    pub fn stronger_than(self, other: Piece) -> bool {
        self.kind > other.kind
    }
}

impl fmt::Display for Piece {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.letter())
    }
}

/// Board square, 0 = a1, 7 = h1, 56 = a8, 63 = h8.
/// Serializes as its index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Square(u8);

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for Square {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Square, D::Error> {
        let index = u8::deserialize(d)?;
        Square::try_from(index).map_err(serde::de::Error::custom)
    }
}

impl TryFrom<u8> for Square {
    type Error = ParseError;

    fn try_from(index: u8) -> Result<Square, ParseError> {
        Square::new(index).ok_or_else(|| ParseError::new(format!("square index {index} out of range")))
    }
}

impl Square {
    pub const fn new(index: u8) -> Option<Square> {
        if index < 64 { Some(Square(index)) } else { None }
    }

    /// `file` and `rank` are 0-based (file 0 = a, rank 0 = 1).
    pub const fn from_coords(file: u8, rank: u8) -> Option<Square> {
        if file < 8 && rank < 8 { Some(Square(rank * 8 + file)) } else { None }
    }

    pub const fn index(self) -> u8 {
        self.0
    }

    pub const fn file(self) -> u8 {
        self.0 % 8
    }

    pub const fn rank(self) -> u8 {
        self.0 / 8
    }

    pub const fn bit(self) -> u64 {
        1u64 << self.0
    }

    pub fn is_trap(self) -> bool {
        TRAPS.contains(&self)
    }

    /// The square one step away in `dir`, if it's on the board.
    pub fn neighbor(self, dir: Dir) -> Option<Square> {
        let (f, r) = (self.file() as i8, self.rank() as i8);
        let (df, dr) = dir.delta();
        let (nf, nr) = (f + df, r + dr);
        if (0..8).contains(&nf) && (0..8).contains(&nr) { Some(Square((nr * 8 + nf) as u8)) } else { None }
    }

    pub fn neighbors(self) -> impl Iterator<Item = Square> {
        Dir::ALL.into_iter().filter_map(move |d| self.neighbor(d))
    }

    /// Direction from `self` to an orthogonally adjacent `to`.
    pub fn direction_to(self, to: Square) -> Option<Dir> {
        Dir::ALL.into_iter().find(|&d| self.neighbor(d) == Some(to))
    }

    pub fn all() -> impl Iterator<Item = Square> {
        (0..64).map(Square)
    }

    pub(crate) fn from_bit_index(i: u32) -> Square {
        Square(i as u8)
    }
}

impl fmt::Display for Square {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", (b'a' + self.file()) as char, self.rank() + 1)
    }
}

impl FromStr for Square {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Square, ParseError> {
        let b = s.as_bytes();
        if b.len() == 2 && (b'a'..=b'h').contains(&b[0]) && (b'1'..=b'8').contains(&b[1]) {
            Ok(Square((b[1] - b'1') * 8 + (b[0] - b'a')))
        } else {
            Err(ParseError::new(format!("invalid square {s:?}")))
        }
    }
}

/// The four trap squares: c3, f3, c6, f6.
pub const TRAPS: [Square; 4] = [Square(18), Square(21), Square(42), Square(45)];

data_type! {
    #[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
    pub enum Dir {
        N,
        S,
        E,
        W,
    }
}

impl Dir {
    pub const ALL: [Dir; 4] = [Dir::N, Dir::S, Dir::E, Dir::W];

    /// (file delta, rank delta)
    pub fn delta(self) -> (i8, i8) {
        match self {
            Dir::N => (0, 1),
            Dir::S => (0, -1),
            Dir::E => (1, 0),
            Dir::W => (-1, 0),
        }
    }

    pub fn letter(self) -> char {
        match self {
            Dir::N => 'n',
            Dir::S => 's',
            Dir::E => 'e',
            Dir::W => 'w',
        }
    }

    pub fn from_letter(c: char) -> Option<Dir> {
        match c {
            'n' => Some(Dir::N),
            's' => Some(Dir::S),
            'e' => Some(Dir::E),
            'w' => Some(Dir::W),
            _ => None,
        }
    }

    pub fn opposite(self) -> Dir {
        match self {
            Dir::N => Dir::S,
            Dir::S => Dir::N,
            Dir::E => Dir::W,
            Dir::W => Dir::E,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn square_roundtrip() {
        for sq in Square::all() {
            assert_eq!(sq.to_string().parse::<Square>().unwrap(), sq);
        }
        assert_eq!("a1".parse::<Square>().unwrap().index(), 0);
        assert_eq!("h8".parse::<Square>().unwrap().index(), 63);
        assert!("i1".parse::<Square>().is_err());
        assert!("a9".parse::<Square>().is_err());
    }

    #[test]
    fn traps() {
        let names: Vec<String> = TRAPS.iter().map(|s| s.to_string()).collect();
        assert_eq!(names, ["c3", "f3", "c6", "f6"]);
    }

    #[test]
    fn neighbors_at_edges() {
        let a1: Square = "a1".parse().unwrap();
        assert_eq!(a1.neighbor(Dir::S), None);
        assert_eq!(a1.neighbor(Dir::W), None);
        assert_eq!(a1.neighbor(Dir::N), Some("a2".parse().unwrap()));
        assert_eq!(a1.neighbor(Dir::E), Some("b1".parse().unwrap()));
        let h8: Square = "h8".parse().unwrap();
        assert_eq!(h8.neighbors().count(), 2);
        let d4: Square = "d4".parse().unwrap();
        assert_eq!(d4.neighbors().count(), 4);
    }

    #[test]
    fn piece_letters() {
        for c in "EMHDCRemhdcr".chars() {
            assert_eq!(Piece::from_letter(c).unwrap().letter(), c);
        }
        assert!(Piece::from_letter('x').is_none());
        assert!(PieceKind::Elephant > PieceKind::Camel);
        assert!(PieceKind::Cat > PieceKind::Rabbit);
    }
}
