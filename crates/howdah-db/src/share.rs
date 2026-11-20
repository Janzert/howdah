//! Sharing: exporting a selection as a database file, and the origins
//! data came from (`docs/DATABASE.md`, "Sharing").
//!
//! An export is a new database in the same format, at this build's
//! schema: the chosen games with what goes with them (variations, the
//! annotations by chosen authors, move times, the chat), their players,
//! and data on positions (none, the positions the games reach, or all of
//! it, by chosen engines). Every row keeps its origin, and the export
//! carries this database's `origins` rows, so whoever imports it
//! ([`Database::import_file`]) knows who wrote what; this database's own
//! row is named with [`ExportOptions::author`].

use std::collections::HashSet;
use std::path::Path;

use rusqlite::params_from_iter;
use rusqlite::types::Value;

use crate::explore::Replay;
use crate::merge::{GAME_COLUMNS, SOURCE_KINDS, prefixed};
use crate::search::conditions;
use crate::{Database, Error, Filter, Result, hex, now};

/// Which games go into an export.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Selection {
    /// The games a filter lets through (the game list's).
    Filter(Box<Filter>),
    /// These games, by id.
    Games(Vec<i64>),
    /// No games (only data on positions).
    None,
}

/// Which data on positions goes into an export.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PositionScope {
    None,
    /// The positions the exported games reach (with their variations,
    /// when those go too).
    Games,
    All,
}

/// What an export holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportOptions {
    pub games: Selection,
    pub variations: bool,
    /// Comments, variation intros, glyphs and kinds this build doesn't
    /// know: by these authors (`''` is this database's user), or all
    /// when `None`.
    pub annotations: bool,
    pub annotation_authors: Option<Vec<String>>,
    /// The move times.
    pub times: bool,
    /// A game's chat (off unless asked for).
    pub chat: bool,
    pub positions: PositionScope,
    /// Engines (authors) whose data on positions goes, or all.
    pub position_authors: Option<Vec<String>>,
    /// The name this database's own data goes under (its origin's name
    /// in the export), such as the user's name.
    pub author: String,
}

impl Default for ExportOptions {
    fn default() -> Self {
        ExportOptions {
            games: Selection::None,
            variations: true,
            annotations: true,
            annotation_authors: None,
            times: true,
            chat: false,
            positions: PositionScope::Games,
            position_authors: None,
            author: String::new(),
        }
    }
}

/// What an export wrote.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExportReport {
    pub games: i64,
    pub annotations: i64,
    pub position_data: i64,
}

/// A database data came from, as [`Database::origins`] lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OriginInfo {
    pub id: i64,
    /// 32 hex digits.
    pub uuid: String,
    pub name: String,
    /// When it was first merged in (Unix seconds).
    pub added_at: i64,
    /// This database itself.
    pub own: bool,
    pub games: i64,
    pub annotations: i64,
    pub position_data: i64,
}

/// What [`Database::remove_origin`] took out.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RemoveReport {
    pub games: i64,
    /// Games from it kept, since they have annotations from elsewhere.
    pub games_kept: i64,
    pub annotations: i64,
    pub position_data: i64,
}

impl Database {
    /// The ids of the games `filter` lets through.
    pub fn filter_ids(&self, filter: &Filter) -> Result<Vec<i64>> {
        if filter.position.is_some() {
            return Ok(self.scan(filter)?.iter().map(|h| h.id).collect());
        }
        let (clauses, params) = conditions(filter);
        let wheres =
            if clauses.is_empty() { String::new() } else { format!("WHERE {}", clauses.join(" AND ")) };
        let mut q = self.connection().prepare(&format!("SELECT id FROM games {wheres}"))?;
        Ok(q.query_map(params_from_iter(params.iter()), |r| r.get(0))?.collect::<rusqlite::Result<_>>()?)
    }

    /// Writes a new database file at `path` holding what `options` choose.
    /// The file mustn't exist yet.
    pub fn export(&self, path: &Path, options: &ExportOptions) -> Result<ExportReport> {
        if path.exists() {
            return Err(Error::Io(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                format!("{} already exists", path.display()),
            )));
        }
        let ids = match &options.games {
            Selection::Filter(f) => self.filter_ids(f)?,
            Selection::Games(ids) => ids.clone(),
            Selection::None => Vec::new(),
        };
        // A fresh database at this schema, then filled from here.
        let dst_uuid = {
            let dst = Database::open(path)?;
            dst.conn.pragma_update(None, "journal_mode", "DELETE")?;
            dst.uuid()?
        };
        let result = (|| {
            self.conn.execute("ATTACH DATABASE ?1 AS dst", [path.to_string_lossy()])?;
            let r = self.export_attached(&ids, options, &dst_uuid);
            let detach = self.conn.execute("DETACH DATABASE dst", []);
            let r = r?;
            detach?;
            Ok(r)
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(path);
        }
        result
    }

    fn export_attached(&self, ids: &[i64], o: &ExportOptions, dst_uuid: &str) -> Result<ExportReport> {
        let tx = self.conn.unchecked_transaction()?;
        let mut report = ExportReport::default();
        tx.execute_batch(
            "CREATE TEMP TABLE xsel (id INTEGER PRIMARY KEY);
             CREATE TEMP TABLE xomap (src INTEGER PRIMARY KEY, dst INTEGER NOT NULL);",
        )?;
        {
            let mut insert = tx.prepare("INSERT OR IGNORE INTO temp.xsel (id) VALUES (?1)")?;
            for id in ids {
                insert.execute([id])?;
            }
        }
        // Origins: every one, this database's named as the author.
        {
            let mut q = tx.prepare("SELECT id, uuid, name, added_at FROM main.origins")?;
            let rows: Vec<(i64, Vec<u8>, String, i64)> = q
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
                .collect::<rusqlite::Result<_>>()?;
            for (id, uuid, name, added) in rows {
                if hex(&uuid) == dst_uuid {
                    continue;
                }
                let name =
                    if id == 1 && !o.author.trim().is_empty() { o.author.trim().to_string() } else { name };
                let dst: i64 = tx.query_row(
                    "INSERT INTO dst.origins (uuid, name, added_at) VALUES (?1, ?2, ?3) RETURNING id",
                    rusqlite::params![uuid, name, added],
                    |r| r.get(0),
                )?;
                tx.execute("INSERT INTO temp.xomap (src, dst) VALUES (?1, ?2)", (id, dst))?;
            }
        }
        let origin = |c: &str| format!("(SELECT dst FROM temp.xomap WHERE src = {c})");
        tx.execute_batch(
            "INSERT INTO dst.players SELECT * FROM main.players WHERE id IN (
               SELECT gold FROM main.games WHERE id IN (SELECT id FROM temp.xsel)
               UNION SELECT silver FROM main.games WHERE id IN (SELECT id FROM temp.xsel));
             INSERT INTO dst.player_names SELECT * FROM main.player_names
               WHERE player IN (SELECT id FROM dst.players);",
        )?;
        report.games = tx.execute(
            &format!(
                "INSERT INTO dst.games (id, origin, gold, silver, {GAME_COLUMNS})
                 SELECT g.id, {}, g.gold, g.silver, {} FROM main.games g
                 WHERE g.id IN (SELECT id FROM temp.xsel)",
                origin("g.origin"),
                prefixed("g", GAME_COLUMNS),
            ),
            [],
        )? as i64;
        tx.execute(
            "INSERT INTO dst.game_tags SELECT * FROM main.game_tags WHERE game IN (SELECT id FROM temp.xsel)",
            [],
        )?;
        if o.variations {
            tx.execute(
                "INSERT INTO dst.nodes SELECT * FROM main.nodes WHERE game IN (SELECT id FROM temp.xsel)",
                [],
            )?;
        }

        // Annotations: the move list always (it's the game's moves when
        // they didn't replay), the rest as chosen.
        let mut kinds = vec!["a.kind = 'movelist'".to_string()];
        if o.times {
            kinds.push("a.kind = 'times'".into());
        }
        if o.chat {
            kinds.push("a.kind = 'chat'".into());
        }
        let mut params: Vec<Value> = Vec::new();
        if o.annotations {
            let source = SOURCE_KINDS.map(|k| format!("'{k}'")).join(", ");
            let mut user = format!("a.kind NOT IN ({source})");
            if let Some(authors) = &o.annotation_authors {
                user += &format!(" AND a.author IN ({})", vec!["?"; authors.len()].join(", "));
                params.extend(authors.iter().map(|a| Value::Text(a.clone())));
            }
            kinds.push(format!("({user})"));
        }
        let nodes = if o.variations { "" } else { " AND a.node >= 0" };
        report.annotations = tx.execute(
            &format!(
                "INSERT INTO dst.annotations (game, node, kind, author, origin, created_at, modified_at, payload)
                 SELECT a.game, a.node, a.kind, a.author, {}, a.created_at, a.modified_at, a.payload
                 FROM main.annotations a WHERE a.game IN (SELECT id FROM temp.xsel){nodes} AND ({})",
                origin("a.origin"),
                kinds.join(" OR "),
            ),
            params_from_iter(params.iter()),
        )? as i64;

        // Data on positions.
        if o.positions != PositionScope::None {
            let mut params: Vec<Value> = Vec::new();
            let mut wheres = Vec::new();
            if let Some(authors) = &o.position_authors {
                wheres.push(format!("p.author IN ({})", vec!["?"; authors.len()].join(", ")));
                params.extend(authors.iter().map(|a| Value::Text(a.clone())));
            }
            if o.positions == PositionScope::Games {
                tx.execute_batch("CREATE TEMP TABLE xpos (hash INTEGER PRIMARY KEY)")?;
                let reached = self.reached(&tx, o.variations, &wheres, &params)?;
                let mut insert = tx.prepare("INSERT INTO temp.xpos (hash) VALUES (?1)")?;
                for h in reached {
                    insert.execute([h as i64])?;
                }
                wheres.push("p.hash IN (SELECT hash FROM temp.xpos)".into());
            }
            let wheres =
                if wheres.is_empty() { String::new() } else { format!("WHERE {}", wheres.join(" AND ")) };
            report.position_data = tx.execute(
                &format!(
                    "INSERT INTO dst.position_data (hash, kind, author, origin, created_at, modified_at, payload)
                     SELECT p.hash, p.kind, p.author, {}, p.created_at, p.modified_at, p.payload
                     FROM main.position_data p {wheres}",
                    origin("p.origin"),
                ),
                params_from_iter(params.iter()),
            )? as i64;
            if o.positions == PositionScope::Games {
                tx.execute_batch("DROP TABLE temp.xpos")?;
            }
        }
        // Only the origins something in the file came from: the others
        // would name files and people that have nothing to do with it.
        tx.execute_batch(
            "DELETE FROM dst.origins WHERE id != 1 AND id NOT IN (
               SELECT origin FROM dst.games UNION SELECT origin FROM dst.annotations
               UNION SELECT origin FROM dst.position_data)",
        )?;
        tx.execute(
            "INSERT INTO dst.meta (key, value) VALUES ('exported_from', ?1), ('exported_at', ?2)",
            (self.uuid()?, now().to_string()),
        )?;
        tx.execute_batch("DROP TABLE temp.xsel; DROP TABLE temp.xomap;")?;
        tx.commit()?;
        Ok(report)
    }

    /// The positions with data (those `wheres` pick) that the selected
    /// games reach. Only positions with data are looked for, so this
    /// needs no set of every position the games pass through.
    fn reached(
        &self,
        tx: &rusqlite::Transaction,
        variations: bool,
        wheres: &[String],
        params: &[Value],
    ) -> Result<HashSet<u64>> {
        let w = if wheres.is_empty() { String::new() } else { format!("WHERE {}", wheres.join(" AND ")) };
        let wanted: HashSet<u64> = {
            let mut q = tx.prepare(&format!("SELECT DISTINCT p.hash FROM main.position_data p {w}"))?;
            q.query_map(params_from_iter(params.iter()), |r| r.get::<_, i64>(0).map(|h| h as u64))?
                .collect::<rusqlite::Result<_>>()?
        };
        let mut found = HashSet::new();
        if wanted.is_empty() {
            return Ok(found);
        }
        // From the setups and with no variations: replayed straight from
        // the blob. Anything else is loaded whole.
        let mut q = tx.prepare(&format!(
            "SELECT g.id, g.moves, g.start_ply = 0 AND NOT ({} AND EXISTS (SELECT 1 FROM main.nodes n WHERE n.game = g.id))
             FROM main.games g WHERE g.id IN (SELECT id FROM temp.xsel) AND g.moves IS NOT NULL",
            if variations { "1" } else { "0" }
        ))?;
        let mut rows = q.query([])?;
        let mut whole = Vec::new();
        while let Some(r) = rows.next()? {
            let id: i64 = r.get(0)?;
            let quick: bool = r.get(2)?;
            if !quick {
                whole.push(id);
                continue;
            }
            let moves: Vec<u8> = r.get(1)?;
            let Some(mut replay) = Replay::new(&moves) else { continue };
            while replay.next_move().is_some() {
                let h = replay.position.hash();
                if wanted.contains(&h) {
                    found.insert(h);
                }
            }
        }
        for id in whole {
            let Ok(game) = self.load(id) else { continue };
            let Some(tree) = game.tree else { continue };
            let mut stack = vec![tree.root()];
            while let Some(n) = stack.pop() {
                let h = tree[n].position().hash();
                if wanted.contains(&h) {
                    found.insert(h);
                }
                let kids = tree[n].children();
                stack.extend(if variations { kids } else { &kids[..kids.len().min(1)] });
            }
        }
        Ok(found)
    }

    /// The databases data here came from, this one first, with how much
    /// of each kind each gave.
    pub fn origins(&self) -> Result<Vec<OriginInfo>> {
        let count = |table: &str| -> Result<std::collections::HashMap<i64, i64>> {
            let mut q =
                self.conn.prepare(&format!("SELECT origin, count(*) FROM {table} GROUP BY origin"))?;
            Ok(q.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?)
        };
        let (games, notes, data) = (count("games")?, count("annotations")?, count("position_data")?);
        let mut q = self.conn.prepare("SELECT id, uuid, name, added_at FROM origins ORDER BY id")?;
        let rows = q.query_map([], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, uuid, name, added_at) = row?;
            out.push(OriginInfo {
                id,
                uuid: hex(&uuid),
                name,
                added_at,
                own: id == 1,
                games: games.get(&id).copied().unwrap_or(0),
                annotations: notes.get(&id).copied().unwrap_or(0),
                position_data: data.get(&id).copied().unwrap_or(0),
            });
        }
        Ok(out)
    }

    /// Renames an origin (not this database's own).
    pub fn rename_origin(&mut self, id: i64, name: &str) -> Result<()> {
        self.writable()?
            .execute("UPDATE origins SET name = ?2 WHERE id = ?1 AND id != 1", (id, name.trim()))?;
        Ok(())
    }

    /// Takes out what came from origin `id`: its games (except those with
    /// annotations from elsewhere, which stay with what came from it
    /// removed), its annotations on other games and its data on
    /// positions. Variations it added to games kept here stay, since
    /// moves don't record where they came from. The origin itself stays
    /// listed, so importing it again is known. This database's own data
    /// (`id` 1) can't be removed this way.
    pub fn remove_origin(&mut self, id: i64) -> Result<RemoveReport> {
        if id == 1 {
            return Err(Error::ReadOnly);
        }
        let tx = self.writable()?.transaction()?;
        let mut report = RemoveReport::default();
        tx.execute_batch(&format!(
            "CREATE TEMP TABLE xkeep AS SELECT DISTINCT game AS id FROM annotations
             WHERE origin != {id} AND game IN (SELECT id FROM games WHERE origin = {id})"
        ))?;
        report.games_kept = tx.query_row("SELECT count(*) FROM temp.xkeep", [], |r| r.get(0))?;
        report.annotations = tx.execute("DELETE FROM annotations WHERE origin = ?1", [id])? as i64;
        report.games = tx
            .execute("DELETE FROM games WHERE origin = ?1 AND id NOT IN (SELECT id FROM temp.xkeep)", [id])?
            as i64;
        // The games kept are this database's now.
        tx.execute("UPDATE games SET origin = 1 WHERE id IN (SELECT id FROM temp.xkeep)", [])?;
        report.position_data = tx.execute("DELETE FROM position_data WHERE origin = ?1", [id])? as i64;
        tx.execute_batch("DROP TABLE temp.xkeep")?;
        tx.commit()?;
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GameData, PositionDatum};
    use howdah_arimaa::GameRecord;

    fn record() -> GameRecord {
        GameRecord::parse(
            "1g Ra1 Rb1 Rc1 Rd1 Re1 Rf1 Rg1 Rh1 Da2 Hb2 Cc2 Ed2 Me2 Hf2 Cg2 Dh2 {[%emt 0:00:05]}\n\
             1s ra8 rb8 rc8 rd8 re8 rf8 rg8 rh8 da7 hb7 cc7 ed7 me7 hf7 cg7 dh7\n2g Ed2n\n2s ed7s\n",
        )
        .unwrap()
    }

    struct Dir(std::path::PathBuf);
    impl Dir {
        fn new(name: &str) -> Dir {
            let d = std::env::temp_dir().join(format!("howdah-share-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&d);
            std::fs::create_dir_all(&d).unwrap();
            Dir(d)
        }
        fn path(&self, f: &str) -> std::path::PathBuf {
            self.0.join(f)
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn comment_at(db: &Database, id: i64, ply: usize) -> Option<String> {
        let g = db.load(id).unwrap();
        let tree = g.tree.unwrap();
        let n = tree.main_line()[ply];
        tree[n].annotation().comment_text()
    }

    fn set_comment(db: &mut Database, id: i64, ply: usize, text: &str) {
        let mut g = db.load(id).unwrap();
        let tree = g.tree.as_mut().unwrap();
        let n = tree.main_line()[ply];
        tree.annotation_mut(n).unwrap().set_comment_text(text);
        db.save(&g).unwrap();
    }

    fn eval(depth: &str) -> Vec<u8> {
        format!(r#"{{"v":1,"depth":"{depth}","eval":{{"cr":5}},"nodes":null,"pv":[],"timeMs":null}}"#)
            .into_bytes()
    }

    fn all(id: i64) -> ExportOptions {
        ExportOptions { games: Selection::Games(vec![id]), author: "Ann".into(), ..Default::default() }
    }

    #[test]
    fn exports_and_imports_with_origins() {
        let dir = Dir::new("roundtrip");
        let mut ann = Database::open(dir.path("ann.db")).unwrap();
        let mut g = GameData::from_record(&record());
        g.chat = Some("hi".into());
        let id = ann.save(&g).unwrap();
        set_comment(&mut ann, id, 3, "Good.");
        let tree = ann.load(id).unwrap().tree.unwrap();
        let end = *tree.main_line().last().unwrap();
        let reached = tree[end].position().hash();
        ann.put_position_data(
            "eval",
            &[
                PositionDatum { hash: reached, author: "E 1".into(), payload: eval("10") },
                PositionDatum { hash: 99, author: "E 1".into(), payload: eval("10") },
            ],
        )
        .unwrap();

        let out = dir.path("share.db");
        let report = ann.export(&out, &all(id)).unwrap();
        assert_eq!(report, ExportReport { games: 1, annotations: 2, position_data: 1 }, "no chat, one eval");
        assert!(ann.export(&out, &all(id)).is_err(), "never over a file");

        let mut bob = Database::open_in_memory().unwrap();
        let r = bob.import_file(&out, "share.db").unwrap();
        assert_eq!((r.games_added, r.position_data_added), (1, 1));
        let got = bob.find_gid(&g.gid).unwrap().unwrap();
        assert_eq!(comment_at(&bob, got, 3).as_deref(), Some("Good."));
        assert!(bob.load(got).unwrap().chat.is_none());
        let origins = bob.origins().unwrap();
        let from_ann = origins.iter().find(|o| o.name == "Ann").expect("Ann's database is named");
        assert_eq!(from_ann.uuid, ann.uuid().unwrap());
        assert_eq!((from_ann.games, from_ann.annotations, from_ann.position_data), (1, 2, 1));
        let filter = Filter { from_db: Some(from_ann.id), ..Default::default() };
        assert_eq!(bob.search(&filter, 0, 10).unwrap().total, 1);

        // Only the evals.
        let evals = ExportOptions { positions: PositionScope::All, ..ExportOptions::default() };
        let out2 = dir.path("evals.db");
        assert_eq!(
            ann.export(&out2, &evals).unwrap(),
            ExportReport { games: 0, annotations: 0, position_data: 2 }
        );
    }

    #[test]
    fn joins_variations_and_keeps_both_edits() {
        let dir = Dir::new("join");
        let mut ann = Database::open(dir.path("ann.db")).unwrap();
        let g = GameData::from_record(&record());
        let id = ann.save(&g).unwrap();
        set_comment(&mut ann, id, 3, "First.");
        set_comment(&mut ann, id, 4, "Mine at 4.");
        ann.export(&dir.path("1.db"), &all(id)).unwrap();
        let mut bob = Database::open_in_memory().unwrap();
        bob.import_file(&dir.path("1.db"), "1").unwrap();
        let b = bob.find_gid(&g.gid).unwrap().unwrap();

        // Ann adds a variation and edits both comments; Bob edits the
        // one at ply 4 too.
        let mut game = ann.load(id).unwrap();
        let tree = game.tree.as_mut().unwrap();
        let after = tree.main_line()[3];
        tree.add_notation(after, "dh7s").unwrap();
        ann.save(&game).unwrap();
        // Edits land a second later, so they're newer.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        set_comment(&mut ann, id, 3, "Second.");
        set_comment(&mut ann, id, 4, "Ann at 4.");
        set_comment(&mut bob, b, 4, "Bob at 4.");
        ann.export(&dir.path("2.db"), &all(id)).unwrap();
        let r = bob.import_file(&dir.path("2.db"), "2").unwrap();
        assert_eq!((r.games_kept, r.nodes_added, r.annotation_conflicts), (1, 1, 1));
        assert_eq!(comment_at(&bob, b, 3).as_deref(), Some("Second."), "Ann's newer edit of her own");
        assert_eq!(comment_at(&bob, b, 4).as_deref(), Some("Bob at 4."), "Bob's stays");
        let kept: String = bob
            .connection()
            .query_row(
                "SELECT author FROM annotations WHERE game = ?1 AND node = 4 AND author != ''",
                [b],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(kept, "Ann");
        let tree = bob.load(b).unwrap().tree.unwrap();
        assert_eq!(tree[tree.main_line()[3]].children().len(), 2, "the variation joined");

        // Again: nothing new.
        let again = bob.import_file(&dir.path("2.db"), "2").unwrap();
        assert_eq!((again.nodes_added, again.annotations_added, again.annotation_conflicts), (0, 0, 0));
    }

    /// Where game `id`'s annotations by `author` are, as (parent reference,
    /// place among siblings) of their variation node.
    fn variation_notes(db: &Database, id: i64, author: &str) -> Vec<(i64, i64, String)> {
        let mut q = db
            .connection()
            .prepare(
                "SELECT n.parent, n.ord, CAST(a.payload AS TEXT) FROM annotations a
                 JOIN nodes n ON n.game = a.game AND n.id = -a.node
                 WHERE a.game = ?1 AND a.author = ?2 AND a.node < 0 ORDER BY n.parent, n.ord",
            )
            .unwrap();
        q.query_map(rusqlite::params![id, author], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    }

    #[test]
    fn other_authors_notes_follow_their_variation_on_save() {
        let dir = Dir::new("renumber");
        let mut ann = Database::open(dir.path("ann.db")).unwrap();
        let g = GameData::from_record(&record());
        let id = ann.save(&g).unwrap();
        // A variation for silver's second move, with a comment.
        let mut game = ann.load(id).unwrap();
        let tree = game.tree.as_mut().unwrap();
        let x = tree.add_notation(tree.main_line()[3], "dh7s").unwrap();
        tree.annotation_mut(x).unwrap().set_comment_text("Ann on X.");
        ann.save(&game).unwrap();
        ann.export(&dir.path("1.db"), &all(id)).unwrap();
        let mut bob = Database::open_in_memory().unwrap();
        bob.import_file(&dir.path("1.db"), "1").unwrap();
        let b = bob.find_gid(&g.gid).unwrap().unwrap();

        // Ann adds a variation for gold's second move, earlier in the
        // tree, and edits X's comment; Bob edits it too.
        let mut game = ann.load(id).unwrap();
        let tree = game.tree.as_mut().unwrap();
        tree.add_notation(tree.main_line()[2], "Hb2n").unwrap();
        ann.save(&game).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let mut game = ann.load(id).unwrap();
        let tree = game.tree.as_mut().unwrap();
        let x = tree[tree.main_line()[3]].children()[1];
        tree.annotation_mut(x).unwrap().set_comment_text("Ann again on X.");
        ann.save(&game).unwrap();
        let mut game = bob.load(b).unwrap();
        let tree = game.tree.as_mut().unwrap();
        let x = tree[tree.main_line()[3]].children()[1];
        tree.annotation_mut(x).unwrap().set_comment_text("Bob on X.");
        bob.save(&game).unwrap();
        ann.export(&dir.path("2.db"), &all(id)).unwrap();
        let r = bob.import_file(&dir.path("2.db"), "2").unwrap();
        assert_eq!((r.nodes_added, r.annotation_conflicts), (1, 1));
        let x_note = vec![(3, 1, "Ann again on X.".to_string())];
        assert_eq!(variation_notes(&bob, b, "Ann"), x_note);

        // The board saves Bob's game with another variation before X.
        let mut record = bob.load(b).unwrap().to_record();
        let tree = &mut record.tree;
        tree.add_notation(tree.main_line()[2], "Cc2n").unwrap();
        bob.save_record(Some(b), &record).unwrap();
        assert_eq!(variation_notes(&bob, b, "Ann"), x_note, "Ann's copy stays on X");
        assert_eq!(variation_notes(&bob, b, ""), vec![(3, 1, "Bob on X.".to_string())]);

        // A variation gone takes the notes on it along.
        let mut game = bob.load(b).unwrap();
        let tree = game.tree.as_mut().unwrap();
        let x = tree[tree.main_line()[3]].children()[1];
        tree.delete(x).unwrap();
        bob.save(&game).unwrap();
        assert_eq!(variation_notes(&bob, b, "Ann"), vec![]);
        let left: i64 = bob
            .connection()
            .query_row("SELECT count(*) FROM annotations WHERE game = ?1 AND author = 'Ann'", [b], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(left, 0);
    }

    #[test]
    fn deeper_evals_win() {
        let dir = Dir::new("evals");
        let mut ann = Database::open(dir.path("ann.db")).unwrap();
        ann.put_position_data(
            "eval",
            &[
                PositionDatum { hash: 1, author: "E".into(), payload: eval("12.5+") },
                PositionDatum { hash: 2, author: "E".into(), payload: eval("6") },
            ],
        )
        .unwrap();
        let mut bob = Database::open_in_memory().unwrap();
        bob.put_position_data(
            "eval",
            &[
                PositionDatum { hash: 1, author: "E".into(), payload: eval("8") },
                PositionDatum { hash: 2, author: "E".into(), payload: eval("9") },
            ],
        )
        .unwrap();
        let opts = ExportOptions { positions: PositionScope::All, ..Default::default() };
        ann.export(&dir.path("e.db"), &opts).unwrap();
        let r = bob.import_file(&dir.path("e.db"), "e").unwrap();
        assert_eq!((r.position_data_added, r.position_data_replaced), (0, 1));
        let got = bob.position_data("eval", &[1, 2], &["E".into()]).unwrap();
        assert_eq!(got[0].payload, eval("12.5+"));
        assert_eq!(got[1].payload, eval("9"));
    }

    #[test]
    fn own_export_comes_back_as_own_and_origins_can_go() {
        let dir = Dir::new("own");
        let mut me = Database::open(dir.path("me.db")).unwrap();
        let g = GameData::from_record(&record());
        let id = me.save(&g).unwrap();
        set_comment(&mut me, id, 3, "Mine.");
        me.export(&dir.path("backup.db"), &all(id)).unwrap();
        me.delete(id).unwrap();
        me.import_file(&dir.path("backup.db"), "backup").unwrap();
        let back = me.find_gid(&g.gid).unwrap().unwrap();
        let origin: i64 = me
            .connection()
            .query_row("SELECT origin FROM annotations WHERE game = ?1 AND kind = 'comment'", [back], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(origin, 1, "this database's own again");

        // Someone else's game, one annotated here, then their data removed.
        let mut ann = Database::open(dir.path("ann.db")).unwrap();
        let mut other = GameData::from_record(&record());
        other.gid = crate::random_id();
        let a1 = ann.save(&other).unwrap();
        let mut third = GameData::from_record(&record());
        third.gid = crate::random_id();
        let a2 = ann.save(&third).unwrap();
        let opts = ExportOptions { games: Selection::Games(vec![a1, a2]), ..Default::default() };
        ann.export(&dir.path("ann-share.db"), &opts).unwrap();
        me.import_file(&dir.path("ann-share.db"), "Ann's games").unwrap();
        let mine_now = me.find_gid(&third.gid).unwrap().unwrap();
        set_comment(&mut me, mine_now, 2, "Noted.");
        let origin = me.origins().unwrap().into_iter().find(|o| o.uuid == ann.uuid().unwrap()).unwrap();
        assert_eq!(origin.name, "Ann's games", "an unnamed origin takes the import's name");
        let r = me.remove_origin(origin.id).unwrap();
        assert_eq!((r.games, r.games_kept), (1, 1));
        assert!(me.find_gid(&other.gid).unwrap().is_none());
        assert_eq!(comment_at(&me, mine_now, 2).as_deref(), Some("Noted."));
        assert!(me.remove_origin(1).is_err());
    }
}
