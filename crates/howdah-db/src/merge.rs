//! Merging another database into this one (the base database, and later
//! databases people share).
//!
//! Games are matched by `gid`, then by source key. A game this database
//! doesn't have is added with its tags, variations and annotations; one it
//! has only the header of gets the other's moves; one it has with its
//! moves keeps them (joining variations and annotations from both is for
//! sharing, later). Players are matched by their site key, else by name.
//! Data on positions is added where this database has none from the same
//! author. Everything added records the other database as its origin.

use std::collections::HashMap;
use std::path::Path;

use rusqlite::{OptionalExtension, params};

use crate::{Database, Error, Result, now, schema};

/// What a merge did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MergeReport {
    pub games_added: i64,
    /// Games known by their header only that got their moves.
    pub games_filled: i64,
    /// Games both had, left as they were.
    pub games_kept: i64,
    pub players_added: i64,
    pub position_data_added: i64,
}

/// The games' columns copied as they are (`id`, `origin` and the players
/// are mapped).
const GAME_COLUMNS: &str = "gid, source, source_id, gold_name, silver_name, gold_rating, silver_rating, \
     gold_rating_k, silver_rating_k, event, site, time_control, rated, postal, date, start_ts, end_ts, \
     winner, termination, plies, start_ply, start_board, rules, moves, fingerprint, replay_error, added_at, \
     modified_at, fetched_at";

impl Database {
    /// Merges the database at `path` into this one. The other file is
    /// brought up to this build's schema first, so it must be one this
    /// build may write (a copy, such as a downloaded base database).
    pub fn merge_from(&mut self, path: &Path, name: &str) -> Result<MergeReport> {
        // Brought up to this schema, so the tables line up.
        let src = Database::open(path)?;
        if schema::version(src.connection())? != schema::VERSION {
            return Err(Error::ReadOnly);
        }
        let src_uuid = src.uuid()?;
        drop(src);
        let uuid = hex_bytes(&src_uuid).ok_or(Error::NotHowdah)?;
        if src_uuid == self.uuid()? {
            return Ok(MergeReport::default());
        }

        let conn = self.writable()?;
        conn.execute("ATTACH DATABASE ?1 AS src", [path.to_string_lossy()])?;
        let result = merge_attached(conn, &uuid, name);
        let detach = conn.execute("DETACH DATABASE src", []);
        let report = result?;
        detach?;
        Ok(report)
    }
}

fn merge_attached(conn: &mut rusqlite::Connection, uuid: &[u8], name: &str) -> Result<MergeReport> {
    let mut report = MergeReport::default();
    let tx = conn.transaction()?;
    let t = now();
    tx.execute(
        "INSERT INTO main.origins (uuid, name, added_at) SELECT ?1, ?2, ?3
         WHERE NOT EXISTS (SELECT 1 FROM main.origins WHERE uuid = ?1)",
        params![uuid, name, t],
    )?;
    let origin: i64 = tx.query_row("SELECT id FROM main.origins WHERE uuid = ?1", [uuid], |r| r.get(0))?;

    // Players, one by one: there are thousands, not hundreds of thousands.
    let mut pmap: HashMap<i64, i64> = HashMap::new();
    {
        let mut players =
            tx.prepare("SELECT id, site, site_id, name, kind, country, title FROM src.players ORDER BY id")?;
        type Row =
            (i64, Option<String>, Option<String>, String, Option<String>, Option<String>, Option<String>);
        let rows: Vec<Row> = players
            .query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?))
            })?
            .collect::<rusqlite::Result<_>>()?;
        let mut by_key = tx.prepare("SELECT id FROM main.players WHERE site = ?1 AND site_id = ?2")?;
        let mut by_name = tx.prepare(
            "SELECT p.id FROM main.player_names n JOIN main.players p ON p.id = n.player
             WHERE n.name = ?1 AND p.site IS ?2 AND (p.site_id IS NULL OR ?3 IS NULL)
             ORDER BY p.id DESC LIMIT 1",
        )?;
        let mut insert = tx.prepare(
            "INSERT INTO main.players (site, site_id, name, kind, country, title)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6) RETURNING id",
        )?;
        for (id, site, site_id, name, kind, country, title) in rows {
            let found: Option<i64> = match (&site, &site_id) {
                (Some(s), Some(k)) => by_key.query_row((s, k), |r| r.get(0)).optional()?,
                _ => None,
            };
            let found = match found {
                Some(f) => Some(f),
                None => by_name.query_row(params![name, site, site_id], |r| r.get(0)).optional()?,
            };
            let dst = match found {
                Some(f) => f,
                None => {
                    report.players_added += 1;
                    insert.query_row(params![site, site_id, name, kind, country, title], |r| r.get(0))?
                }
            };
            pmap.insert(id, dst);
        }
    }
    tx.execute_batch("CREATE TEMP TABLE pmap (src INTEGER PRIMARY KEY, dst INTEGER NOT NULL)")?;
    {
        let mut insert = tx.prepare("INSERT INTO temp.pmap (src, dst) VALUES (?1, ?2)")?;
        for (s, d) in &pmap {
            insert.execute((s, d))?;
        }
    }
    tx.execute(
        "INSERT OR IGNORE INTO main.player_names (player, name)
         SELECT m.dst, n.name FROM src.player_names n JOIN temp.pmap m ON m.src = n.player",
        [],
    )?;

    // Games: `how` is 0 for one kept, 1 for one filled in, 2 for one added
    // (with an id past this database's highest, so no lookup is needed).
    tx.execute_batch(
        "CREATE TEMP TABLE gmap (src INTEGER PRIMARY KEY, dst INTEGER NOT NULL, how INTEGER NOT NULL);
         INSERT INTO temp.gmap SELECT s.id, g.id, (g.moves IS NULL AND s.moves IS NOT NULL)
           FROM src.games s JOIN main.games g ON g.gid = s.gid;
         INSERT OR IGNORE INTO temp.gmap SELECT s.id, g.id, (g.moves IS NULL AND s.moves IS NOT NULL)
           FROM src.games s JOIN main.games g ON g.source = s.source AND g.source_id = s.source_id
           WHERE s.source IS NOT NULL;",
    )?;
    let top: i64 = tx.query_row("SELECT coalesce(max(id), 0) FROM main.games", [], |r| r.get(0))?;
    tx.execute(
        "INSERT INTO temp.gmap SELECT s.id, ?1 + s.id, 2 FROM src.games s
         WHERE s.id NOT IN (SELECT src FROM temp.gmap)",
        [top],
    )?;
    let player = |side: &str| format!("(SELECT dst FROM temp.pmap WHERE src = s.{side})");
    report.games_added = tx.execute(
        &format!(
            "INSERT INTO main.games (id, origin, gold, silver, {GAME_COLUMNS})
             SELECT m.dst, ?1, {}, {}, {}
             FROM src.games s JOIN temp.gmap m ON m.src = s.id WHERE m.how = 2",
            player("gold"),
            player("silver"),
            prefixed("s", GAME_COLUMNS),
        ),
        [origin],
    )? as i64;
    report.games_filled = tx.execute(
        &format!(
            "UPDATE main.games SET (gold, silver, {GAME_COLUMNS_NO_GID}) =
               (SELECT {}, {}, {} FROM src.games s WHERE s.id = m.src)
             FROM temp.gmap AS m WHERE m.dst = games.id AND m.how = 1",
            player("gold"),
            player("silver"),
            prefixed("s", GAME_COLUMNS_NO_GID),
        ),
        [],
    )? as i64;
    report.games_kept = tx.query_row("SELECT count(*) FROM temp.gmap WHERE how = 0", [], |r| r.get(0))?;
    tx.execute_batch(
        "INSERT INTO main.game_tags (game, ord, name, value)
           SELECT m.dst, t.ord, t.name, t.value FROM src.game_tags t JOIN temp.gmap m ON m.src = t.game
           WHERE m.how = 2;
         INSERT INTO main.nodes (game, id, parent, ord, move)
           SELECT m.dst, n.id, n.parent, n.ord, n.move FROM src.nodes n JOIN temp.gmap m ON m.src = n.game
           WHERE m.how = 2;",
    )?;
    tx.execute(
        "INSERT OR IGNORE INTO main.annotations
           (game, node, kind, author, origin, created_at, modified_at, payload)
         SELECT m.dst, a.node, a.kind, a.author, ?1, a.created_at, a.modified_at, a.payload
         FROM src.annotations a JOIN temp.gmap m ON m.src = a.game WHERE m.how > 0",
        [origin],
    )?;
    report.position_data_added = tx.execute(
        "INSERT OR IGNORE INTO main.position_data
           (hash, kind, author, origin, created_at, modified_at, payload)
         SELECT hash, kind, author, ?1, created_at, modified_at, payload FROM src.position_data",
        [origin],
    )? as i64;
    tx.execute_batch("DROP TABLE temp.pmap; DROP TABLE temp.gmap;")?;
    tx.commit()?;
    Ok(report)
}

/// The columns copied when filling in a game known by its header (all but
/// its `gid`, which it keeps).
const GAME_COLUMNS_NO_GID: &str = "source, source_id, gold_name, silver_name, gold_rating, silver_rating, \
     gold_rating_k, silver_rating_k, event, site, time_control, rated, postal, date, start_ts, end_ts, \
     winner, termination, plies, start_ply, start_board, rules, moves, fingerprint, replay_error, added_at, \
     modified_at, fetched_at";

fn prefixed(table: &str, columns: &str) -> String {
    columns.split(',').map(|c| format!("{table}.{}", c.trim())).collect::<Vec<_>>().join(", ")
}

fn hex_bytes(hex: &str) -> Option<Vec<u8>> {
    (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GameData, PlayerInfo, PositionDatum, SourceKey, gid_for};
    use howdah_arimaa::GameRecord;

    fn record(extra: &str) -> GameRecord {
        GameRecord::parse(&format!(
            "1g Ra1 Rb1 Rc1 Rd1 Re1 Rf1 Rg1 Rh1 Da2 Hb2 Cc2 Ed2 Me2 Hf2 Cg2 Dh2 {{[%emt 0:00:05]}}\n\
             1s ra8 rb8 rc8 rd8 re8 rf8 rg8 rh8 da7 hb7 cc7 ed7 me7 hf7 cg7 dh7\n2g Ed2n\n{extra}"
        ))
        .unwrap()
    }

    fn arimaa(id: &str, gold: &str, site_id: &str, header_only: bool) -> GameData {
        let mut g = GameData::from_record(&record(""));
        g.source = Some(SourceKey::arimaa_com(id));
        g.gid = gid_for(g.source.as_ref().unwrap());
        g.players[0] =
            PlayerInfo { name: Some(gold.into()), site_id: Some(site_id.into()), ..Default::default() };
        g.tags.push(("Mode".into(), "IGS".into()));
        if header_only {
            g.tree = None;
        }
        g
    }

    #[test]
    fn merges_new_games_fills_headers_and_keeps_its_own() {
        let dir = std::env::temp_dir().join(format!("howdah-merge-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let base_path = dir.join("base.db");
        let _ = std::fs::remove_file(&base_path);
        {
            let mut base = Database::open(&base_path).unwrap();
            base.save(&arimaa("1", "Ann", "10", false)).unwrap();
            base.save(&arimaa("2", "Bob", "20", false)).unwrap();
            base.save(&arimaa("3", "Ann", "10", false)).unwrap();
            base.put_position_data(
                "eval",
                &[PositionDatum { hash: 5, author: "E 1".into(), payload: b"base".to_vec() }],
            )
            .unwrap();
        }
        let mut main = Database::open_in_memory().unwrap();
        // The user's own game, a header-only copy of game 2, and game 3
        // with their own analysis.
        let own = main.save(&GameData::from_record(&record("2s ed7s\n"))).unwrap();
        let header = main.save(&arimaa("2", "Bob", "20", true)).unwrap();
        let mut mine = arimaa("3", "Ann", "10", false);
        let tree = mine.tree.as_mut().unwrap();
        let end = *tree.main_line().last().unwrap();
        tree.annotation_mut(end).unwrap().set_comment_text("Mine.");
        let kept = main.save(&mine).unwrap();
        main.put_position_data(
            "eval",
            &[PositionDatum { hash: 5, author: "E 1".into(), payload: b"mine".to_vec() }],
        )
        .unwrap();

        let report = main.merge_from(&base_path, "base").unwrap();
        assert_eq!(
            report,
            MergeReport {
                games_added: 1,
                games_filled: 1,
                games_kept: 1,
                players_added: 0,
                position_data_added: 0
            }
        );
        assert_eq!(main.game_count().unwrap(), 4);
        let added = main.find_source("arimaa.com", "1").unwrap().unwrap();
        let g = main.load(added).unwrap();
        assert_eq!(g.players[0].site_id.as_deref(), Some("10"));
        assert_eq!(g.tags, [("Mode".to_string(), "IGS".to_string())]);
        assert!(g.to_record().to_record().contains("[%emt 0:00:05]"), "annotations come too");
        assert!(main.has_moves(header).unwrap());
        let kept_game = main.load(kept).unwrap();
        let end = *kept_game.tree.as_ref().unwrap().main_line().last().unwrap();
        assert_eq!(kept_game.tree.unwrap()[end].annotation().comment_text().as_deref(), Some("Mine."));
        assert!(main.load(own).is_ok());
        let ann: i64 = main
            .connection()
            .query_row("SELECT count(*) FROM players WHERE name = 'Ann'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(ann, 1, "players are matched by their site id");
        let evals = main.position_data("eval", &[5], &["E 1".into()]).unwrap();
        assert_eq!(evals[0].payload, b"mine", "ours stays");

        // A second merge adds nothing.
        let again = main.merge_from(&base_path, "base").unwrap();
        assert_eq!((again.games_added, again.games_filled, again.games_kept), (0, 0, 3));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
