//! The setup explorer: statistics on the games' setups.
//!
//! A [`Setup`] is one side's setup seen from gold's side of the board:
//! its 16 squares in the order a1, b1, …, h1, a2, …, h2, with silver's
//! ranks 8 and 7 read as 1 and 2, so the same setup has the same key for
//! either color. The board is the same mirrored left to right, so a setup
//! and its mirror image count as one, written with the elephant on files
//! e–h (where it more often is: gold's is on d2 and e2 about equally,
//! silver's more often on e7). Silver's reply to a gold setup is mirrored
//! with gold's, so it keeps its place relative to gold's pieces.
//!
//! [`Database::setup_stats`] groups the games a [`Filter`] lets through
//! by gold's setup, by silver's reply to one gold setup, or by silver's
//! setup alone. The SQL function `howdah_setup(moves, start_ply, which)`
//! gives the same keys inside a query (`which` is `'g'`, `'s'` or `'r'`,
//! the reply), for [`Filter::gold_setup`] and [`Filter::silver_setup`].
//! It's registered on this crate's connections only and is never part of
//! the schema, so other tools can still read and write the file.

use std::collections::HashMap;

use howdah_arimaa::PieceKind;
use rusqlite::functions::FunctionFlags;
use rusqlite::{Connection, types::ValueRef};

use crate::codec::FORMAT;
use crate::search::{Filter, conditions};
use crate::{Database, Result};

/// One side's setup, seen from gold's side: a nibble per square (0 for
/// empty, else the piece kind's index plus 1), a1 … h1, a2 … h2.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Setup(pub [u8; 16]);

const ELEPHANT: u8 = PieceKind::Elephant as u8 + 1;

impl Setup {
    /// From a setup's 8 bytes in the move codec (the mover's home squares
    /// in square order).
    fn from_codec(bytes: &[u8], silver: bool) -> Setup {
        let mut n = [0u8; 16];
        for (i, slot) in n.iter_mut().enumerate() {
            *slot = bytes[i / 2] >> (4 * (i % 2)) & 0xF;
        }
        if silver {
            // Silver's squares run a7 … h7, a8 … h8: rank 7 is its front.
            n.rotate_left(8);
        }
        Setup(n)
    }

    /// The setup mirrored left to right.
    pub fn mirror(self) -> Setup {
        let mut n = self.0;
        n[..8].reverse();
        n[8..].reverse();
        Setup(n)
    }

    /// Whether this is the way round the explorer writes a setup: the
    /// elephant on files e–h (or, with no elephant, the smaller of the two).
    fn is_canonical(self) -> bool {
        match self.0.iter().position(|&k| k == ELEPHANT) {
            Some(i) => i % 8 >= 4,
            None => self <= self.mirror(),
        }
    }

    pub fn canonical(self) -> Setup {
        if self.is_canonical() { self } else { self.mirror() }
    }

    /// The setup as 16 letters (`RCDHME`, `.` for an empty square), a1 …
    /// h1 then a2 … h2.
    pub fn key(self) -> String {
        self.0
            .iter()
            .map(|&k| match k {
                0 => '.',
                k => PieceKind::ALL.get(k as usize - 1).map_or('?', |p| p.letter()),
            })
            .collect()
    }

    /// The setup's pieces for `color` (for silver, the key's first row is
    /// rank 8 and its second rank 7).
    pub fn placements(self, color: howdah_arimaa::Color) -> Vec<howdah_arimaa::Placement> {
        let silver = color == howdah_arimaa::Color::Silver;
        self.0
            .iter()
            .enumerate()
            .filter_map(|(i, &k)| {
                let kind = *PieceKind::ALL.get((k as usize).checked_sub(1)?)?;
                let rank = match (silver, i < 8) {
                    (false, true) => 0,
                    (false, false) => 1,
                    (true, true) => 7,
                    (true, false) => 6,
                };
                Some(howdah_arimaa::Placement {
                    piece: howdah_arimaa::Piece::new(color, kind),
                    square: howdah_arimaa::Square::new(rank * 8 + (i % 8) as u8)?,
                })
            })
            .collect()
    }

    /// Reads a [`Setup::key`].
    pub fn parse(key: &str) -> Option<Setup> {
        let mut n = [0u8; 16];
        let mut chars = key.trim().chars();
        for slot in n.iter_mut() {
            let c = chars.next()?.to_ascii_uppercase();
            *slot = if c == '.' { 0 } else { PieceKind::ALL.iter().position(|p| p.letter() == c)? as u8 + 1 };
        }
        chars.next().is_none().then_some(Setup(n))
    }
}

/// Both setups of a game from the setups (not a set position), if it has
/// both.
fn setups(moves: &[u8], start_ply: i64) -> Option<(Setup, Setup)> {
    if start_ply != 0 || moves.len() < 17 || moves[0] != FORMAT {
        return None;
    }
    Some((Setup::from_codec(&moves[1..9], false), Setup::from_codec(&moves[9..17], true)))
}

/// The keys the explorer groups a game by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Keys {
    gold: Setup,
    silver: Setup,
    /// Silver's setup mirrored as gold's was.
    reply: Setup,
}

fn keys(moves: &[u8], start_ply: i64) -> Option<Keys> {
    let (g, s) = setups(moves, start_ply)?;
    let (gold, reply) = if g.is_canonical() { (g, s) } else { (g.mirror(), s.mirror()) };
    Some(Keys { gold, silver: s.canonical(), reply })
}

/// Registers `howdah_setup` on a connection.
pub(crate) fn register(conn: &Connection) -> rusqlite::Result<()> {
    conn.create_scalar_function(
        "howdah_setup",
        3,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            let ValueRef::Blob(moves) = ctx.get_raw(0) else { return Ok(None) };
            let start_ply: i64 = ctx.get(1)?;
            let which: String = ctx.get(2)?;
            Ok(keys(moves, start_ply).and_then(|k| match which.as_str() {
                "g" => Some(k.gold.key()),
                "s" => Some(k.silver.key()),
                "r" => Some(k.reply.key()),
                _ => None,
            }))
        },
    )
}

/// What the explorer groups by.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SetupLevel {
    /// Gold's setups.
    Gold,
    /// Silver's replies to one gold setup (a [`Setup::key`]).
    Replies(String),
    /// Silver's setups, whatever gold's was.
    Silver,
}

/// The games with one setup (or pair).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SetupStat {
    /// The setup's [`Setup::key`].
    pub setup: String,
    pub games: i64,
    pub gold_wins: i64,
    pub silver_wins: i64,
    /// Games with both players' ratings, gold's wins among them, and the
    /// wins gold's rating predicted (the sum of each game's expected score).
    pub rated_games: i64,
    pub rated_gold_wins: i64,
    pub expected_gold_wins: f64,
    /// The average rating of the side playing the setup (gold's for gold
    /// setups, silver's for replies and silver setups), over games that
    /// have it.
    pub rating: Option<f64>,
    /// Unix seconds: the newest game.
    pub last: i64,
    /// The newest game's date, `YYYY.MM.DD` (UTC).
    pub last_date: String,
}

/// The explorer's rows, most played first.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SetupStats {
    pub rows: Vec<SetupStat>,
    /// Games counted in all (those with both setups), and distinct setups.
    pub games: i64,
    pub setups: i64,
}

/// Gold's expected score from the ratings (Elo).
pub(crate) fn expected(gold: i64, silver: i64) -> f64 {
    1.0 / (1.0 + 10f64.powf((silver - gold) as f64 / 400.0))
}

#[derive(Default)]
struct Acc {
    stat: SetupStat,
    rating_sum: f64,
    rating_n: i64,
}

impl Database {
    /// The games `filter` lets through, grouped at `level`, the `limit`
    /// most played setups.
    pub fn setup_stats(&self, filter: &Filter, level: &SetupLevel, limit: usize) -> Result<SetupStats> {
        let (mut clauses, params) = conditions(filter);
        clauses.push("moves IS NOT NULL AND start_ply = 0".into());
        let sql = format!(
            "SELECT moves, winner, gold_rating, silver_rating, coalesce(start_ts, end_ts, added_at), id
             FROM games WHERE {}",
            clauses.join(" AND ")
        );
        let only_gold = match level {
            SetupLevel::Replies(g) => Some(Setup::parse(g).map(Setup::canonical)),
            _ => None,
        };
        let mut groups: HashMap<Setup, Acc> = HashMap::new();
        let mut q = self.connection().prepare(&sql)?;
        let mut rows = q.query(rusqlite::params_from_iter(params.iter()))?;
        // A position in the filter: only the games reaching it.
        let reaching: Option<std::collections::HashSet<i64>> = match filter.position {
            Some(_) => Some(self.scan(filter)?.iter().map(|h| h.id).collect()),
            None => None,
        };
        let mut games = 0;
        while let Some(r) = rows.next()? {
            if let Some(ids) = &reaching
                && !ids.contains(&r.get(5)?)
            {
                continue;
            }
            let ValueRef::Blob(moves) = r.get_ref(0)? else { continue };
            let Some(k) = keys(moves, 0) else { continue };
            let setup = match (level, only_gold) {
                (SetupLevel::Gold, _) => k.gold,
                (SetupLevel::Silver, _) => k.silver,
                (SetupLevel::Replies(_), Some(Some(g))) if g == k.gold => k.reply,
                (SetupLevel::Replies(_), _) => continue,
            };
            games += 1;
            let winner: Option<String> = r.get(1)?;
            let ratings: [Option<i64>; 2] = [r.get(2)?, r.get(3)?];
            let when: i64 = r.get(4)?;
            let acc = groups.entry(setup).or_default();
            let s = &mut acc.stat;
            s.games += 1;
            let gold_won = winner.as_deref() == Some("g");
            s.gold_wins += i64::from(gold_won);
            s.silver_wins += i64::from(winner.as_deref() == Some("s"));
            if let [Some(g), Some(sr)] = ratings
                && g > 0
                && sr > 0
            {
                s.rated_games += 1;
                s.rated_gold_wins += i64::from(gold_won);
                s.expected_gold_wins += expected(g, sr);
            }
            let mine = if matches!(level, SetupLevel::Gold) { ratings[0] } else { ratings[1] };
            if let Some(m) = mine.filter(|&m| m > 0) {
                acc.rating_sum += m as f64;
                acc.rating_n += 1;
            }
            s.last = s.last.max(when);
        }
        let setups = groups.len() as i64;
        let mut all: Vec<(Setup, Acc)> = groups.into_iter().collect();
        all.sort_by(|a, b| b.1.stat.games.cmp(&a.1.stat.games).then(a.0.cmp(&b.0)));
        let rows = all
            .into_iter()
            .take(limit)
            .map(|(setup, acc)| SetupStat {
                setup: setup.key(),
                rating: (acc.rating_n > 0).then(|| acc.rating_sum / acc.rating_n as f64),
                last_date: crate::game::utc_date(acc.stat.last),
                ..acc.stat
            })
            .collect();
        Ok(SetupStats { rows, games, setups })
    }
}

#[cfg(test)]
mod tests {
    use howdah_arimaa::GameRecord;

    use super::*;
    use crate::GameData;

    const GOLD: &str = "1g Ra1 Rb1 Rc1 Cd1 Ce1 Rf1 Rg1 Rh1 Ra2 Hb2 Dc2 Md2 Ee2 Df2 Hg2 Rh2";
    /// The same, mirrored.
    const GOLD_M: &str = "1g Ra1 Rb1 Rc1 Cd1 Ce1 Rf1 Rg1 Rh1 Ra2 Hb2 Dc2 Ed2 Me2 Df2 Hg2 Rh2";
    const SILVER: &str = "1s ra8 rb8 rc8 rd8 re8 rf8 rg8 rh8 ha7 db7 cc7 ed7 me7 cf7 dg7 hh7";

    fn game(gold: &str, silver: &str, winner: char, ratings: (i64, i64)) -> GameData {
        let record = GameRecord::parse(&format!("{gold}\n{silver}\n")).unwrap();
        let mut g = GameData::from_record(&record);
        g.players[0].rating = Some(ratings.0);
        g.players[1].rating = Some(ratings.1);
        g.result = Some(crate::StoredResult { winner, termination: Some('r') });
        g
    }

    #[test]
    fn keys_merge_mirrors_and_keep_replies_relative() {
        let s = Setup::parse("RRRCCRRRRHDMEDHR").unwrap();
        assert_eq!(s.key(), "RRRCCRRRRHDMEDHR");
        assert!(s.is_canonical(), "elephant on e2");
        assert_eq!(s.mirror().key(), "RRRCCRRRRHDEMDHR");
        assert_eq!(s.mirror().canonical(), s);
        assert_eq!(Setup::parse("RRR"), None);
        assert_eq!(Setup::parse("RRRCCRRRRHDMEDHRX"), None);
    }

    #[test]
    fn stats_by_level() {
        let mut db = Database::open_in_memory().unwrap();
        db.save(&game(GOLD, SILVER, 'g', (1600, 1400))).unwrap();
        db.save(&game(GOLD_M, SILVER, 's', (1500, 1500))).unwrap();
        db.save(&game(GOLD, SILVER, 's', (0, 1500))).unwrap();

        let gold = db.setup_stats(&Filter::default(), &SetupLevel::Gold, 10).unwrap();
        assert_eq!((gold.games, gold.setups), (3, 1), "a setup and its mirror are one");
        let row = &gold.rows[0];
        assert_eq!(row.setup, "RRRCCRRRRHDMEDHR");
        assert_eq!(
            (row.games, row.gold_wins, row.silver_wins, row.rated_games, row.rated_gold_wins),
            (3, 1, 2, 2, 1)
        );
        assert!((row.expected_gold_wins - (expected(1600, 1400) + 0.5)).abs() < 1e-9);
        assert_eq!(row.rating, Some(1550.0));

        // Silver's setup has its elephant on d7; seen from gold's side and
        // with the elephant on e–h, it's this.
        let silver = db.setup_stats(&Filter::default(), &SetupLevel::Silver, 10).unwrap();
        assert_eq!(
            silver.rows.iter().map(|r| (r.setup.as_str(), r.games)).collect::<Vec<_>>(),
            [("RRRRRRRRHDCMECDH", 3)]
        );

        // Against the mirrored gold setup the same silver setup is the
        // other reply.
        let replies =
            db.setup_stats(&Filter::default(), &SetupLevel::Replies("RRRCCRRRRHDEMDHR".into()), 10).unwrap();
        let mut got: Vec<(String, i64)> = replies.rows.iter().map(|r| (r.setup.clone(), r.games)).collect();
        got.sort();
        assert_eq!(got, [("RRRRRRRRHDCEMCDH".into(), 2), ("RRRRRRRRHDCMECDH".into(), 1)]);
        assert_eq!(replies.rows[0].rating, Some(1450.0), "silver's, in games 1 and 3");
    }

    #[test]
    fn filter_by_setup() {
        let mut db = Database::open_in_memory().unwrap();
        db.save(&game(GOLD, SILVER, 'g', (1600, 1400))).unwrap();
        db.save(&game(GOLD_M, SILVER, 's', (1500, 1500))).unwrap();
        let by = |gold: Option<&str>, silver: Option<&str>| {
            let f = Filter {
                gold_setup: gold.map(String::from),
                silver_setup: silver.map(String::from),
                ..Default::default()
            };
            db.search(&f, 0, 10).unwrap().total
        };
        assert_eq!(by(Some("RRRCCRRRRHDMEDHR"), None), 2);
        assert_eq!(by(Some("rrrccrrrrhdemdhr"), None), 2, "either way round, any case");
        assert_eq!(by(None, Some("RRRRRRRRHDCMECDH")), 2);
        assert_eq!(by(Some("RRRCCRRRRHDMEDHR"), Some("RRRRRRRRHDCMECDH")), 1);
        assert_eq!(by(Some("RRRCCRRRRHDMEDHR"), Some("RRRRRRRRHDCEMCDH")), 1);
        assert_eq!(by(Some("nonsense"), None), 0);
    }
}
