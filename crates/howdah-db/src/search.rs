//! Finding games by their headers: the game list.
//!
//! A [`Filter`] narrows the games by players, dates, where they're from,
//! rated or not and how they ended; [`Database::search`] gives a page of
//! [`GameSummary`]s, newest first (by start time, else end time, else
//! when it was added), and how many games match.

use rusqlite::types::Value;

use crate::game::{ARIMAA_COM, utc_date};
use crate::{Database, Result};

/// Where a game is from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// arimaa.com's archive or a game fetched from it.
    ArimaaCom,
    /// Games from the app: played, analysed, records opened.
    Local,
}

/// How a game ended for the [`Filter::player`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Won,
    Lost,
}

/// What games to list. Every field set narrows the list.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Filter {
    /// A player, by any name they've had (case doesn't matter).
    pub player: Option<String>,
    /// Their opponent, the same way (or either player, without `player`).
    pub opponent: Option<String>,
    /// The player's side: 'g' or 's'.
    pub side: Option<char>,
    /// Unix seconds, from (inclusive) and to (exclusive).
    pub from: Option<i64>,
    pub to: Option<i64>,
    pub origin: Option<Origin>,
    pub rated: Option<bool>,
    /// For the player (needs `player`).
    pub outcome: Option<Outcome>,
    /// The ending's letter (`g`, `e`, `m`, `t`, `r`, ...).
    pub termination: Option<char>,
}

/// A game as the list shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameSummary {
    pub id: i64,
    /// arimaa.com's permanent id, for its games.
    pub arimaa_id: Option<String>,
    pub gold: Option<String>,
    pub silver: Option<String>,
    pub gold_rating: Option<i64>,
    pub silver_rating: Option<i64>,
    /// `YYYY.MM.DD`: the game's date, else when it started, else when it
    /// was added.
    pub date: String,
    pub event: Option<String>,
    pub time_control: Option<String>,
    pub rated: Option<bool>,
    pub postal: Option<bool>,
    /// 'g', 's', 'd' or 'u'.
    pub winner: Option<char>,
    pub termination: Option<char>,
    /// Moves on the main line, setups included.
    pub plies: i64,
    /// Only the header is here (a game from a list, not yet fetched).
    pub header_only: bool,
    /// From a set position rather than the setups.
    pub set_position: bool,
    /// Has variations.
    pub variations: bool,
}

/// A page of games and how many match in all.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchPage {
    pub games: Vec<GameSummary>,
    pub total: i64,
}

/// The players with a name, any case.
const PLAYERS_NAMED: &str = "SELECT player FROM player_names WHERE name = ? COLLATE NOCASE";

impl Database {
    /// The games `filter` lets through, newest first, `limit` of them
    /// after skipping `offset`, and how many there are.
    pub fn search(&self, filter: &Filter, offset: i64, limit: i64) -> Result<SearchPage> {
        let mut clauses: Vec<String> = Vec::new();
        let mut params: Vec<Value> = Vec::new();
        let named = |name: &str, params: &mut Vec<Value>| {
            params.push(Value::Text(name.trim().to_string()));
            format!("({PLAYERS_NAMED})")
        };
        let player = filter.player.as_deref().filter(|p| !p.trim().is_empty());
        let opponent = filter.opponent.as_deref().filter(|p| !p.trim().is_empty());
        match player {
            Some(p) => {
                // The player's side and the opponent's are tied together.
                let mut sides = Vec::new();
                for (me, them, letter) in [("gold", "silver", 'g'), ("silver", "gold", 's')] {
                    if filter.side.is_some_and(|s| s != letter) {
                        continue;
                    }
                    let mut parts = vec![format!("{me} IN {}", named(p, &mut params))];
                    if let Some(o) = opponent {
                        parts.push(format!("{them} IN {}", named(o, &mut params)));
                    }
                    match filter.outcome {
                        Some(Outcome::Won) => parts.push(format!("winner = '{letter}'")),
                        Some(Outcome::Lost) => {
                            parts.push(format!("winner = '{}'", if letter == 'g' { 's' } else { 'g' }))
                        }
                        None => {}
                    }
                    sides.push(format!("({})", parts.join(" AND ")));
                }
                clauses.push(if sides.is_empty() { "0".into() } else { format!("({})", sides.join(" OR ")) });
            }
            None => {
                if let Some(o) = opponent {
                    let a = named(o, &mut params);
                    let b = named(o, &mut params);
                    clauses.push(format!("(gold IN {a} OR silver IN {b})"));
                }
            }
        }
        if let Some(from) = filter.from {
            clauses.push("coalesce(start_ts, end_ts, added_at) >= ?".into());
            params.push(Value::Integer(from));
        }
        if let Some(to) = filter.to {
            clauses.push("coalesce(start_ts, end_ts, added_at) < ?".into());
            params.push(Value::Integer(to));
        }
        match filter.origin {
            Some(Origin::ArimaaCom) => {
                clauses.push("source = ?".into());
                params.push(Value::Text(ARIMAA_COM.into()));
            }
            Some(Origin::Local) => clauses.push("source IS NULL".into()),
            None => {}
        }
        if let Some(rated) = filter.rated {
            clauses.push(if rated { "rated = 1" } else { "coalesce(rated, 0) = 0" }.into());
        }
        if let Some(t) = filter.termination {
            clauses.push("termination = ?".into());
            params.push(Value::Text(t.to_string()));
        }
        let wheres =
            if clauses.is_empty() { String::new() } else { format!("WHERE {}", clauses.join(" AND ")) };
        let total: i64 = self.connection().query_row(
            &format!("SELECT count(*) FROM games {wheres}"),
            rusqlite::params_from_iter(params.iter()),
            |r| r.get(0),
        )?;
        let sql = format!(
            "SELECT id, source, source_id, gold_name, silver_name, gold_rating, silver_rating, date,
             coalesce(start_ts, end_ts, added_at), event, time_control, rated, postal, winner, termination, plies,
             moves IS NULL, start_ply > 0, EXISTS (SELECT 1 FROM nodes n WHERE n.game = games.id)
             FROM games {wheres}
             ORDER BY coalesce(start_ts, end_ts, added_at) DESC, id DESC LIMIT ? OFFSET ?"
        );
        params.push(Value::Integer(limit));
        params.push(Value::Integer(offset));
        let mut q = self.connection().prepare(&sql)?;
        let ch = |s: Option<String>| s.and_then(|s| s.chars().next());
        let games = q
            .query_map(rusqlite::params_from_iter(params.iter()), |r| {
                let source: Option<String> = r.get(1)?;
                let date: Option<String> = r.get(7)?;
                let when: i64 = r.get(8)?;
                Ok(GameSummary {
                    id: r.get(0)?,
                    arimaa_id: if source.as_deref() == Some(ARIMAA_COM) { r.get(2)? } else { None },
                    gold: r.get(3)?,
                    silver: r.get(4)?,
                    gold_rating: r.get(5)?,
                    silver_rating: r.get(6)?,
                    date: date.unwrap_or_else(|| utc_date(when)),
                    event: r.get(9)?,
                    time_control: r.get(10)?,
                    rated: r.get(11)?,
                    postal: r.get(12)?,
                    winner: ch(r.get(13)?),
                    termination: ch(r.get(14)?),
                    plies: r.get(15)?,
                    header_only: r.get(16)?,
                    set_position: r.get(17)?,
                    variations: r.get(18)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(SearchPage { games, total })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GameData, PlayerInfo, SourceKey, StoredResult, gid_for};

    fn game(gold: &str, silver: &str, start: i64, winner: char, arimaa: Option<&str>) -> GameData {
        let mut g = GameData::new();
        g.players[0] = PlayerInfo { name: Some(gold.into()), ..Default::default() };
        g.players[1] = PlayerInfo { name: Some(silver.into()), ..Default::default() };
        g.start_ts = Some(start);
        g.result = Some(StoredResult { winner, termination: Some('g') });
        g.rated = Some(arimaa.is_some());
        if let Some(id) = arimaa {
            g.source = Some(SourceKey::arimaa_com(id));
            g.gid = gid_for(g.source.as_ref().unwrap());
        }
        g
    }

    fn names(page: &SearchPage) -> Vec<(String, String)> {
        page.games.iter().map(|g| (g.gold.clone().unwrap(), g.silver.clone().unwrap())).collect()
    }

    #[test]
    fn filters_and_order() {
        let mut db = Database::open_in_memory().unwrap();
        db.save(&game("Ann", "Bob", 100, 'g', Some("1"))).unwrap();
        db.save(&game("Bob", "Cy", 200, 'g', Some("2"))).unwrap();
        db.save(&game("Cy", "Ann", 300, 's', None)).unwrap();
        let all = db.search(&Filter::default(), 0, 10).unwrap();
        assert_eq!(all.total, 3);
        assert_eq!(names(&all)[0], ("Cy".into(), "Ann".into()), "newest first");
        assert_eq!(all.games[2].arimaa_id.as_deref(), Some("1"));
        assert_eq!(all.games[2].date, "1970.01.01");

        let ann = Filter { player: Some("ann".into()), ..Default::default() };
        assert_eq!(db.search(&ann, 0, 10).unwrap().total, 2);
        let won = Filter { outcome: Some(Outcome::Won), ..ann.clone() };
        assert_eq!(db.search(&won, 0, 10).unwrap().total, 2);
        let gold = Filter { side: Some('g'), ..ann.clone() };
        assert_eq!(names(&db.search(&gold, 0, 10).unwrap()), [("Ann".into(), "Bob".into())]);
        let vs = Filter { opponent: Some("Cy".into()), ..ann.clone() };
        assert_eq!(names(&db.search(&vs, 0, 10).unwrap()), [("Cy".into(), "Ann".into())]);
        let either = Filter { opponent: Some("Bob".into()), ..Default::default() };
        assert_eq!(db.search(&either, 0, 10).unwrap().total, 2);

        let local = Filter { origin: Some(Origin::Local), ..Default::default() };
        assert_eq!(db.search(&local, 0, 10).unwrap().total, 1);
        let rated = Filter { rated: Some(true), ..Default::default() };
        assert_eq!(db.search(&rated, 0, 10).unwrap().total, 2);
        let dates = Filter { from: Some(150), to: Some(300), ..Default::default() };
        assert_eq!(names(&db.search(&dates, 0, 10).unwrap()), [("Bob".into(), "Cy".into())]);

        let page = db.search(&Filter::default(), 1, 1).unwrap();
        assert_eq!((page.total, names(&page)), (3, vec![("Bob".into(), "Cy".into())]));
    }
}
