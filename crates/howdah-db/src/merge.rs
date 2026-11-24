//! Merging another database into this one: the base database, and
//! databases people share ([`crate::share`] writes them).
//!
//! - **Origins.** Everything keeps the database it was first written in:
//!   the other file's `origins` are matched to this one's by uuid (added
//!   when new), and each game, annotation and datum keeps its own. So a
//!   file passed on twice still names who wrote what, and this
//!   database's own data coming back (from an export of it) is its own
//!   again.
//! - **Games** are matched by `gid`, then by source key, and records
//!   from a file ([`Database::import_records`]), whose ids are made up as
//!   they're read, also by their moves' fingerprint and players' names.
//!   A game this
//!   database doesn't have is added with its tags, variations and
//!   annotations; one it has only the header of gets the other's moves.
//!   One both have keeps its headers, taking the other's only where its
//!   own are empty, and the two trees are joined: a
//!   node is its moves from the start, so a node both have is the same
//!   node, and the other's new moves are added as variations.
//! - **Annotations** (by node, kind and author) are added where this
//!   database has none. Where both have one that differs, the newer edit
//!   (the other's on a tie) wins if both come from the same origin; otherwise this one's stays
//!   and the other's is kept beside it, under its origin's name as the
//!   author ([`conflict_author`]). The kinds a source gives (times, chat,
//!   a move list) are never replaced.
//! - **Lines** (variations) keep their origin too. A game here may
//!   already have lines from the import's own source, such as an earlier
//!   analysis of it by the same engine, that the import's don't include:
//!   the import keeps them, replaces them, or stops without changing
//!   anything to ask which ([`EarlierLines`], [`Earlier`]). Replacing
//!   removes only lines wholly from that source: a line the user has
//!   extended or commented on stays.
//! - **Players** are matched by their site key, else by name.
//! - **Data on positions** is added where this database has none from
//!   the same author, and replaces this one's when it's better: for an
//!   eval the deeper search, then the newer; for other kinds the newer.
//!
//! Nothing is deleted by a merge, except earlier lines it's asked to
//! replace.

use std::collections::HashMap;
use std::path::Path;

use rusqlite::functions::FunctionFlags;
use rusqlite::{OptionalExtension, Transaction, params};

use crate::{Database, Error, Result, codec, hex, now, schema};

/// What a merge did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MergeReport {
    pub games_added: i64,
    /// Games known by their header only that got their moves.
    pub games_filled: i64,
    /// Games both had, kept with their headers (their trees joined).
    pub games_kept: i64,
    /// Games both had whose empty headers (event, site, ...) the other's
    /// filled in.
    pub games_completed: i64,
    /// Moves added as variations to games both had.
    pub nodes_added: i64,
    /// Annotations added to games both had, or replacing older edits.
    pub annotations_added: i64,
    /// Annotations kept beside this database's under another author,
    /// since both had changed them.
    pub annotation_conflicts: i64,
    pub players_added: i64,
    pub position_data_added: i64,
    /// Data on positions replaced by better data.
    pub position_data_replaced: i64,
    /// The games here whose trees or annotations a join changed (by id),
    /// so a board showing one can take the joined game in.
    pub games_joined: Vec<i64>,
    /// Games here with lines from the import's source that the import's
    /// own lines don't include ([`EarlierLines`]).
    pub earlier: Vec<Earlier>,
    /// Nothing was merged: there were earlier lines, and the import was
    /// asked to stop and say so ([`EarlierLines::Ask`]).
    pub undecided: bool,
    /// Moves of earlier lines removed ([`EarlierLines::Replace`]).
    pub lines_removed: i64,
}

/// What a merge does with lines a game here already has from the same
/// source as the import's (an earlier analysis by the same engine, say)
/// where the import's own lines don't include them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EarlierLines {
    /// Keeps them beside the new ones.
    #[default]
    Keep,
    /// Removes them, except where the user has extended or commented on
    /// one, and adds the new ones.
    Replace,
    /// Changes nothing if there are any, reporting them
    /// ([`MergeReport::undecided`]), so the caller can ask which.
    Ask,
}

/// A game here with earlier lines from the import's source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Earlier {
    pub game: i64,
    /// The source's name, such as the engine that analysed the game.
    pub source: String,
    /// The moves that replacing would remove.
    pub moves: i64,
}

/// The games' columns copied as they are (`id`, `origin` and the players
/// are mapped).
pub(crate) const GAME_COLUMNS: &str = "gid, source, source_id, gold_name, silver_name, gold_rating, \
     silver_rating, gold_rating_k, silver_rating_k, event, site, time_control, rated, postal, date, \
     start_ts, end_ts, winner, termination, plies, start_ply, start_board, rules, moves, fingerprint, \
     replay_error, added_at, modified_at, fetched_at";

/// The annotation kinds a source gives, which are the game's own data
/// rather than anyone's commentary.
pub(crate) const SOURCE_KINDS: [&str; 3] = ["times", "chat", "movelist"];

impl Database {
    /// Merges the database at `path` into this one. The other file is
    /// brought up to this build's schema first, so it must be one this
    /// build may write (a copy, such as a downloaded base database; see
    /// [`Database::import_file`] for a file to leave as it is). `name`
    /// names the other file's own origin when this database doesn't know
    /// it yet.
    pub fn merge_from(&mut self, path: &Path, name: &str) -> Result<MergeReport> {
        self.merge_with(path, name, false, EarlierLines::Keep)
    }

    /// [`Database::merge_from`], also matching games by fingerprint when
    /// `fingerprints` (for records, whose games have no lasting ids).
    fn merge_with(
        &mut self,
        path: &Path,
        name: &str,
        fingerprints: bool,
        earlier: EarlierLines,
    ) -> Result<MergeReport> {
        self.merge_inner(path, name, fingerprints, earlier, false)
    }

    /// [`Database::merge_with`]; `dry` only looks, changing nothing.
    fn merge_inner(
        &mut self,
        path: &Path,
        name: &str,
        fingerprints: bool,
        earlier: EarlierLines,
        dry: bool,
    ) -> Result<MergeReport> {
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
        conn.create_scalar_function(
            "howdah_better",
            5,
            FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
            |ctx| {
                let kind: String = ctx.get(0)?;
                let new: Vec<u8> = ctx.get(1)?;
                let new_t: i64 = ctx.get(2)?;
                let old: Vec<u8> = ctx.get(3)?;
                let old_t: i64 = ctx.get(4)?;
                Ok(better(&kind, (&new, new_t), (&old, old_t)))
            },
        )?;
        conn.execute("ATTACH DATABASE ?1 AS src", [path.to_string_lossy()])?;
        let result = merge_attached(conn, &uuid, name, fingerprints, earlier, dry);
        let detach = conn.execute("DETACH DATABASE src", []);
        let report = result?;
        detach?;
        Ok(report)
    }

    /// Imports a database file someone shared (or an export of this one),
    /// leaving the file as it is: it's copied beside this database (or
    /// to the temporary directory), brought up to this schema there,
    /// merged in, and the copy removed.
    pub fn import_file(&mut self, path: &Path, name: &str) -> Result<MergeReport> {
        self.import_file_with(path, name, EarlierLines::Keep)
    }

    /// [`Database::import_file`], doing `earlier` with lines from the
    /// file's sources that a game here has and the file's don't include.
    pub fn import_file_with(
        &mut self,
        path: &Path,
        name: &str,
        earlier: EarlierLines,
    ) -> Result<MergeReport> {
        let conn = rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        match schema::check(&conn)? {
            schema::Compat::NewerReadable(_) => return Err(Error::ReadOnly),
            schema::Compat::Older(0) => return Err(Error::NotHowdah),
            _ => {}
        }
        let copy = self.scratch_path();
        conn.execute("VACUUM INTO ?1", [copy.to_string_lossy()])?;
        drop(conn);
        let result = self.merge_with(&copy, name, false, earlier);
        remove_scratch(&copy);
        result
    }
}

impl Database {
    /// Imports game records (a record file's games): saved into a
    /// database of their own beside this one, then merged in as
    /// [`Database::import_file`] does, so a game already here (by its
    /// `GameId`, or the same moves and players) gets the record's
    /// variations and comments joined into it rather than a copy. `name`
    /// names their origin, such as the file's name.
    pub fn import_records(
        &mut self,
        records: &[howdah_arimaa::GameRecord],
        name: &str,
    ) -> Result<MergeReport> {
        self.import_records_with(records, name, EarlierLines::Keep)
    }

    /// [`Database::import_records`], doing `earlier` with lines a game
    /// here already has from the same source. Records naming an
    /// `Annotator` (an engine's analysis) come from that annotator,
    /// whichever file they're in, so a newer analysis by the same one
    /// finds the lines of the last; other records come from the file.
    /// With [`EarlierLines::Ask`] nothing is imported if any game has
    /// earlier lines.
    pub fn import_records_with(
        &mut self,
        records: &[howdah_arimaa::GameRecord],
        name: &str,
        earlier: EarlierLines,
    ) -> Result<MergeReport> {
        // The records by source: each annotator's, then the rest.
        let mut groups: Vec<(Option<String>, Vec<&howdah_arimaa::GameRecord>)> = Vec::new();
        for r in records {
            let who = r.tag("Annotator").map(str::trim).filter(|a| !a.is_empty()).map(str::to_string);
            match groups.iter_mut().find(|(a, _)| *a == who) {
                Some((_, rs)) => rs.push(r),
                None => groups.push((who, vec![r])),
            }
        }
        if earlier == EarlierLines::Ask {
            // Asked first, every group, changing nothing.
            let mut found = MergeReport::default();
            for (who, rs) in &groups {
                let r = self.import_group(rs, who.as_deref(), name, EarlierLines::Ask, true)?;
                found.earlier.extend(r.earlier);
            }
            if !found.earlier.is_empty() {
                found.undecided = true;
                return Ok(found);
            }
        }
        let how = if earlier == EarlierLines::Ask { EarlierLines::Keep } else { earlier };
        let mut report = MergeReport::default();
        for (who, rs) in &groups {
            report.add(self.import_group(rs, who.as_deref(), name, how, false)?);
        }
        Ok(report)
    }

    /// Imports records from one source: `annotator`'s, or the file's.
    fn import_group(
        &mut self,
        records: &[&howdah_arimaa::GameRecord],
        annotator: Option<&str>,
        name: &str,
        earlier: EarlierLines,
        dry: bool,
    ) -> Result<MergeReport> {
        let copy = self.scratch_path();
        let result = (|| {
            {
                let mut tmp = Database::open(&copy)?;
                let tx = tmp.writable()?.transaction()?;
                if let Some(who) = annotator {
                    let uuid = annotator_uuid(who);
                    tx.execute("UPDATE meta SET value = ?1 WHERE key = 'uuid'", [hex(&uuid)])?;
                    tx.execute("UPDATE origins SET uuid = ?1 WHERE id = 1", [uuid.as_slice()])?;
                }
                let mut cache = crate::game::PlayerCache::default();
                for r in records {
                    crate::game::save(
                        &tx,
                        &crate::GameData::from_record(r),
                        &mut cache,
                        crate::SaveMode::User,
                    )?;
                }
                tx.commit()?;
            }
            self.merge_inner(&copy, annotator.unwrap_or(name), true, earlier, dry)
        })();
        remove_scratch(&copy);
        result
    }

    /// A new file beside this database (or in the temporary directory).
    fn scratch_path(&self) -> std::path::PathBuf {
        let dir = match self.conn.path().filter(|p| !p.is_empty()) {
            Some(p) => Path::new(p).parent().map(Path::to_path_buf).unwrap_or_else(std::env::temp_dir),
            None => std::env::temp_dir(),
        };
        dir.join(format!("import-{}.db", hex(&crate::random_id()[..8])))
    }
}

/// The origin of an annotator's records, the same in every database:
/// from its name, so each analysis by one engine comes from one source.
fn annotator_uuid(annotator: &str) -> [u8; 16] {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(format!("howdah annotator\0{annotator}").as_bytes());
    let mut id = [0u8; 16];
    id.copy_from_slice(&digest[..16]);
    id
}

impl MergeReport {
    /// Adds another merge's counts to this one's.
    fn add(&mut self, o: MergeReport) {
        self.games_added += o.games_added;
        self.games_filled += o.games_filled;
        self.games_kept += o.games_kept;
        self.games_completed += o.games_completed;
        self.nodes_added += o.nodes_added;
        self.annotations_added += o.annotations_added;
        self.annotation_conflicts += o.annotation_conflicts;
        self.players_added += o.players_added;
        self.position_data_added += o.position_data_added;
        self.position_data_replaced += o.position_data_replaced;
        self.games_joined.extend(o.games_joined);
        self.earlier.extend(o.earlier);
        self.undecided |= o.undecided;
        self.lines_removed += o.lines_removed;
    }
}

fn remove_scratch(path: &Path) {
    for ext in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
    }
}

/// Whether `new` (a payload and when it was written) is better data on a
/// position than `old`: for an eval, from a deeper search, else as deep
/// and newer; for other kinds, newer.
fn better(kind: &str, new: (&[u8], i64), old: (&[u8], i64)) -> bool {
    if kind == "eval" {
        let (a, b) = (eval_depth(new.0), eval_depth(old.0));
        if a != b {
            return a > b;
        }
    }
    new.1 > old.1
}

/// An eval's depth, as the number its `depth` starts with (Sharp writes
/// `12.0233+`); none sorts first.
fn eval_depth(payload: &[u8]) -> f64 {
    let depth = serde_json::from_slice::<serde_json::Value>(payload)
        .ok()
        .and_then(|v| v.get("depth").and_then(|d| d.as_str().map(str::to_string)));
    let Some(depth) = depth else { return f64::NEG_INFINITY };
    let end = depth.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(depth.len());
    depth[..end].parse().unwrap_or(f64::NEG_INFINITY)
}

fn merge_attached(
    conn: &mut rusqlite::Connection,
    uuid: &[u8],
    name: &str,
    fingerprints: bool,
    earlier: EarlierLines,
    dry: bool,
) -> Result<MergeReport> {
    let mut report = MergeReport::default();
    let tx = conn.transaction()?;
    let t = now();

    // Origins: the other file's own, and every one it has data from.
    tx.execute_batch("CREATE TEMP TABLE omap (src INTEGER PRIMARY KEY, dst INTEGER NOT NULL)")?;
    {
        let mut rows = tx.prepare("SELECT id, uuid, name FROM src.origins")?;
        let rows: Vec<(i64, Vec<u8>, String)> = rows
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let mut has_own = false;
        for (id, u, n) in rows {
            // Unnamed origins take the name the import was given.
            let n = if n.trim().is_empty() { name.to_string() } else { n };
            has_own |= u == uuid;
            let dst = origin_for(&tx, &u, &n, t)?;
            tx.execute("INSERT INTO temp.omap (src, dst) VALUES (?1, ?2)", (id, dst))?;
        }
        if !has_own {
            origin_for(&tx, uuid, name, t)?;
        }
    }
    let origin: i64 = tx.query_row("SELECT id FROM main.origins WHERE uuid = ?1", [uuid], |r| r.get(0))?;
    let mapped =
        |column: &str| format!("coalesce((SELECT dst FROM temp.omap WHERE src = {column}), {origin})");
    let omap: HashMap<i64, i64> = {
        let mut q = tx.prepare("SELECT src, dst FROM temp.omap")?;
        q.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?
    };

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

    // Games: `how` is 0 for one both have, 1 for one filled in, 2 for one
    // added (with an id past this database's highest, so no lookup is
    // needed).
    tx.execute_batch(
        "CREATE TEMP TABLE gmap (src INTEGER PRIMARY KEY, dst INTEGER NOT NULL, how INTEGER NOT NULL);
         INSERT INTO temp.gmap SELECT s.id, g.id, (g.moves IS NULL AND s.moves IS NOT NULL)
           FROM src.games s JOIN main.games g ON g.gid = s.gid;
         INSERT OR IGNORE INTO temp.gmap SELECT s.id, g.id, (g.moves IS NULL AND s.moves IS NOT NULL)
           FROM src.games s JOIN main.games g ON g.source = s.source AND g.source_id = s.source_id
           WHERE s.source IS NOT NULL;",
    )?;
    if fingerprints {
        // One game here for each record at most: the newest that matches.
        tx.execute_batch(
            "INSERT OR IGNORE INTO temp.gmap
             SELECT s.id, (SELECT g.id FROM main.games g
               WHERE g.fingerprint = s.fingerprint AND g.source IS NULL
                 AND g.gold_name IS s.gold_name AND g.silver_name IS s.silver_name
                 AND g.id NOT IN (SELECT dst FROM temp.gmap)
               ORDER BY g.id DESC LIMIT 1), 0
             FROM src.games s
             WHERE s.fingerprint IS NOT NULL AND s.source IS NULL
               AND s.id NOT IN (SELECT src FROM temp.gmap)
               AND EXISTS (SELECT 1 FROM main.games g WHERE g.fingerprint = s.fingerprint AND g.source IS NULL
                 AND g.gold_name IS s.gold_name AND g.silver_name IS s.silver_name);",
        )?;
    }
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
             SELECT m.dst, {}, {}, {}, {}
             FROM src.games s JOIN temp.gmap m ON m.src = s.id WHERE m.how = 2",
            mapped("s.origin"),
            player("gold"),
            player("silver"),
            prefixed("s", GAME_COLUMNS),
        ),
        [],
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
    // A game both have keeps its headers, but takes the other's where its
    // own are empty: a game fetched before it reached the archive lacks
    // what only the archive has (event, site, times), and gets it from the
    // base database later.
    let empty =
        FILLED_HEADERS.iter().map(|c| format!("games.{c} IS NULL AND s.{c} IS NOT NULL")).collect::<Vec<_>>();
    report.games_completed = tx.execute(
        &format!(
            "UPDATE main.games SET {}
             FROM temp.gmap AS m JOIN src.games AS s ON s.id = m.src
             WHERE m.dst = games.id AND m.how = 0 AND ({})",
            FILLED_HEADERS
                .iter()
                .map(|c| format!("{c} = coalesce(games.{c}, s.{c})"))
                .collect::<Vec<_>>()
                .join(", "),
            empty.join(" OR "),
        ),
        [],
    )? as i64;
    tx.execute_batch(&format!(
        "INSERT INTO main.game_tags (game, ord, name, value)
           SELECT m.dst, t.ord, t.name, t.value FROM src.game_tags t JOIN temp.gmap m ON m.src = t.game
           WHERE m.how = 2;
         INSERT INTO main.nodes (game, id, parent, ord, move, origin)
           SELECT m.dst, n.id, n.parent, n.ord, n.move, {} FROM src.nodes n JOIN temp.gmap m ON m.src = n.game
           WHERE m.how = 2;",
        mapped("n.origin"),
    ))?;
    tx.execute(
        &format!(
            "INSERT OR IGNORE INTO main.annotations
               (game, node, kind, author, origin, created_at, modified_at, payload)
             SELECT m.dst, a.node, a.kind, a.author, {}, a.created_at, a.modified_at, a.payload
             FROM src.annotations a JOIN temp.gmap m ON m.src = a.game WHERE m.how > 0",
            mapped("a.origin"),
        ),
        [],
    )?;

    // Games both had with their moves: joined where the other has
    // anything this one lacks (most merges of the base have nothing).
    let joins: Vec<(i64, i64)> = {
        let mut q = tx.prepare(
            "SELECT m.src, m.dst FROM temp.gmap m
             JOIN src.games s ON s.id = m.src JOIN main.games g ON g.id = m.dst
             WHERE m.how = 0 AND s.moves IS NOT NULL AND g.moves IS NOT NULL
               AND (s.moves != g.moves
                 OR EXISTS (SELECT 1 FROM src.nodes n WHERE n.game = m.src)
                 OR EXISTS (SELECT 1 FROM src.annotations a WHERE a.game = m.src AND NOT EXISTS (
                      SELECT 1 FROM main.annotations b WHERE b.game = m.dst AND b.node = a.node
                        AND b.kind = a.kind AND b.author = a.author AND b.payload = a.payload)))",
        )?;
        q.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?
    };
    for (s, d) in joins {
        if join_game(&tx, s, d, origin, &omap, earlier, &mut report)? {
            report.games_joined.push(d);
        }
    }
    // Asked to stop at earlier lines, or only looking: nothing changes.
    if dry || (earlier == EarlierLines::Ask && !report.earlier.is_empty()) {
        report.undecided = !report.earlier.is_empty();
        let found = std::mem::take(&mut report.earlier);
        tx.rollback()?;
        return Ok(MergeReport { earlier: found, undecided: report.undecided, ..MergeReport::default() });
    }

    report.position_data_replaced = tx.execute(
        &format!(
            "UPDATE main.position_data AS d
             SET payload = s.payload, modified_at = s.modified_at, origin = {}
             FROM src.position_data AS s
             WHERE d.hash = s.hash AND d.kind = s.kind AND d.author = s.author AND d.payload != s.payload
               AND howdah_better(d.kind, s.payload, s.modified_at, d.payload, d.modified_at)",
            mapped("s.origin"),
        ),
        [],
    )? as i64;
    report.position_data_added = tx.execute(
        &format!(
            "INSERT OR IGNORE INTO main.position_data
               (hash, kind, author, origin, created_at, modified_at, payload)
             SELECT hash, kind, author, {}, created_at, modified_at, payload FROM src.position_data",
            mapped("origin"),
        ),
        [],
    )? as i64;
    tx.execute_batch("DROP TABLE temp.pmap; DROP TABLE temp.gmap; DROP TABLE temp.omap;")?;
    tx.commit()?;
    Ok(report)
}

/// This database's origin with `uuid`, added if it's new (named `name`),
/// or named `name` if it had no name yet.
fn origin_for(tx: &Transaction, uuid: &[u8], name: &str, t: i64) -> Result<i64> {
    tx.execute(
        "INSERT INTO main.origins (uuid, name, added_at) SELECT ?1, ?2, ?3
         WHERE NOT EXISTS (SELECT 1 FROM main.origins WHERE uuid = ?1)",
        params![uuid, name, t],
    )?;
    tx.execute(
        "UPDATE main.origins SET name = ?2 WHERE uuid = ?1 AND name = '' AND id != 1 AND ?2 != ''",
        params![uuid, name],
    )?;
    Ok(tx.query_row("SELECT id FROM main.origins WHERE uuid = ?1", [uuid], |r| r.get(0))?)
}

/// The author an annotation from `origin` by `author` is kept under when
/// it differs from this database's: its origin's name (or, unnamed, a
/// short form of its uuid), after the author if it had one.
fn conflict_author(tx: &Transaction, origin: i64, author: &str) -> Result<String> {
    let (uuid, name): (Vec<u8>, String) =
        tx.query_row("SELECT uuid, name FROM main.origins WHERE id = ?1", [origin], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?;
    let from = if name.trim().is_empty() { format!("import {}", &hex(&uuid)[..8]) } else { name };
    Ok(if author.is_empty() { from } else { format!("{author} ({from})") })
}

/// Joins the other database's game `s` into this one's game `d`: its moves
/// this game lacks become variations, and its annotations are merged.
/// Joins the other database's game `s` into this one's `d`: its new
/// moves as variations (from their origins, `fallback` where the other
/// has none), and its annotations. Lines `d` has from the same sources
/// that `s`'s don't include are earlier lines: reported, and kept or
/// removed as `earlier` says; with [`EarlierLines::Ask`] the game is
/// left alone when it has any. Returns whether the game was joined.
fn join_game(
    tx: &Transaction,
    s: i64,
    d: i64,
    fallback: i64,
    omap: &HashMap<i64, i64>,
    earlier: EarlierLines,
    report: &mut MergeReport,
) -> Result<bool> {
    type Line = (Vec<u8>, i64, Option<String>);
    let line = |table: &str, id: i64| -> Result<Line> {
        Ok(tx.query_row(
            &format!("SELECT moves, start_ply, start_board FROM {table}.games WHERE id = ?1"),
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?)
    };
    let (s_moves, s_start, s_board) = line("src", s)?;
    let (d_moves, d_start, d_board) = line("main", d)?;
    if (s_start, &s_board) != (d_start, &d_board) {
        return Ok(true);
    }
    let s_main = codec::split_line(&s_moves, s_start as usize).map_err(|e| Error::Moves(d, e))?;
    let d_main = codec::split_line(&d_moves, d_start as usize).map_err(|e| Error::Moves(d, e))?;
    let same_line = s_moves == d_moves;
    let mapped = |o: Option<i64>| o.and_then(|o| omap.get(&o).copied()).unwrap_or(fallback);

    // The other game's variations (id, parent, move, origin here).
    type Row = (i64, i64, Vec<u8>, i64);
    let s_rows: Vec<Row> = {
        let mut q =
            tx.prepare("SELECT id, parent, move, origin FROM src.nodes WHERE game = ?1 ORDER BY id")?;
        q.query_map([s], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get::<_, Option<i64>>(3)?)))?
            .map(|r| r.map(|(id, p, mv, o)| (id, p, mv, mapped(o))))
            .collect::<rusqlite::Result<_>>()?
    };
    // This game's (id, parent, ord, move, origin; none is this database's).
    type Here = (i64, i64, i64, Vec<u8>, Option<i64>);
    let mut d_rows: Vec<Here> = {
        let mut q =
            tx.prepare("SELECT id, parent, ord, move, origin FROM main.nodes WHERE game = ?1 ORDER BY id")?;
        q.query_map([d], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?
            .collect::<rusqlite::Result<_>>()?
    };

    // Earlier lines: this game's from the other's sources (its lines' and
    // comments'; never this database's own) that its lines don't reach.
    let mut sources: std::collections::HashSet<i64> = s_rows.iter().map(|r| r.3).collect();
    {
        let source = SOURCE_KINDS.map(|k| format!("'{k}'")).join(", ");
        let mut q = tx.prepare(&format!(
            "SELECT DISTINCT origin FROM src.annotations WHERE game = ?1 AND kind NOT IN ({source})"
        ))?;
        for o in q.query_map([s], |r| r.get::<_, Option<i64>>(0))? {
            sources.insert(mapped(o?));
        }
    }
    sources.remove(&1);
    let from_sources = |o: Option<i64>| o.is_some_and(|o| sources.contains(&o));
    if d_rows.iter().any(|r| from_sources(r.4)) {
        // What the other's lines reach here, by moves from the start.
        let mut edges: HashMap<(i64, &[u8]), i64> = HashMap::new();
        for (i, mv) in d_main.iter().enumerate() {
            edges.insert((i as i64, mv), i as i64 + 1);
        }
        for (id, parent, _, mv, _) in &d_rows {
            edges.insert((*parent, mv.as_slice()), -id);
        }
        let mut reached: HashMap<i64, i64> = HashMap::from([(0, 0)]);
        let s_refs = s_main.iter().enumerate().map(|(i, mv)| (i as i64 + 1, i as i64, *mv));
        let s_vars = s_rows.iter().map(|(id, parent, mv, _)| (-id, *parent, mv.as_slice()));
        let mut hit: std::collections::HashSet<i64> = std::collections::HashSet::new();
        for (r, parent, mv) in s_refs.chain(s_vars) {
            if let Some(&at) = reached.get(&parent).and_then(|p| edges.get(&(*p, mv))) {
                reached.insert(r, at);
                hit.insert(at);
            }
        }
        // Removable: from those sources, not reached, nothing on it from
        // anyone else, and every move after it removable too.
        let others: std::collections::HashSet<i64> = {
            let mut q =
                tx.prepare("SELECT node, origin FROM main.annotations WHERE game = ?1 AND node < 0")?;
            q.query_map([d], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?
                .filter_map(|r| match r {
                    Ok((node, o)) if !sources.contains(&o) => Some(Ok(node)),
                    Ok(_) => None,
                    Err(e) => Some(Err(e)),
                })
                .collect::<rusqlite::Result<_>>()?
        };
        let mut children: HashMap<i64, Vec<i64>> = HashMap::new();
        for (id, parent, ..) in &d_rows {
            children.entry(*parent).or_default().push(-id);
        }
        let mut removable: std::collections::HashSet<i64> = std::collections::HashSet::new();
        let mut by_source: HashMap<i64, i64> = HashMap::new();
        for (id, _, _, _, origin) in d_rows.iter().rev() {
            let r = -id;
            let ok = from_sources(*origin)
                && !hit.contains(&r)
                && !others.contains(&r)
                && children.get(&r).is_none_or(|kids| kids.iter().all(|k| removable.contains(k)));
            if ok {
                removable.insert(r);
                *by_source.entry(origin.unwrap_or(1)).or_default() += 1;
            }
        }
        if !removable.is_empty() {
            let (&o, _) = by_source.iter().max_by_key(|(o, n)| (**n, -**o)).expect("a source");
            let source: String =
                tx.query_row("SELECT name FROM main.origins WHERE id = ?1", [o], |r| r.get(0))?;
            report.earlier.push(Earlier { game: d, source, moves: removable.len() as i64 });
            match earlier {
                EarlierLines::Ask => return Ok(false),
                EarlierLines::Keep => {}
                EarlierLines::Replace => {
                    let mut drop_notes =
                        tx.prepare("DELETE FROM main.annotations WHERE game = ?1 AND node = ?2")?;
                    let mut drop_node = tx.prepare("DELETE FROM main.nodes WHERE game = ?1 AND id = ?2")?;
                    for r in &removable {
                        drop_notes.execute(params![d, r])?;
                        drop_node.execute(params![d, -r])?;
                    }
                    report.lines_removed += removable.len() as i64;
                    d_rows.retain(|row| !removable.contains(&-row.0));
                }
            }
        }
    }

    // This game's nodes by their parent and move, and the next place
    // under each node (for a new variation's).
    let mut edges: HashMap<(i64, Vec<u8>), i64> = HashMap::new();
    let mut next_ord: HashMap<i64, i64> = HashMap::new();
    for (i, mv) in d_main.iter().enumerate() {
        edges.insert((i as i64, mv.to_vec()), i as i64 + 1);
        next_ord.insert(i as i64, 1);
    }
    let mut last_id = 0i64;
    for (id, parent, ord, mv, _) in &d_rows {
        edges.insert((*parent, mv.clone()), -id);
        let n = next_ord.entry(*parent).or_default();
        *n = (*n).max(ord + 1);
        last_id = last_id.max(*id);
    }
    {
        // Ids stay above any removed, as before.
        let max: Option<i64> =
            tx.query_row("SELECT max(id) FROM main.nodes WHERE game = ?1", [d], |r| r.get(0))?;
        last_id = last_id.max(max.unwrap_or(0));
    }
    let mut insert = tx.prepare(
        "INSERT INTO main.nodes (game, id, parent, ord, move, origin) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )?;
    let mut place = |parent: i64, mv: &[u8], origin: i64| -> Result<i64> {
        if let Some(&r) = edges.get(&(parent, mv.to_vec())) {
            return Ok(r);
        }
        last_id += 1;
        let ord = next_ord.entry(parent).or_default();
        let origin = if origin == 1 { None } else { Some(origin) };
        insert.execute(params![d, last_id, parent, *ord, mv, origin])?;
        *ord += 1;
        edges.insert((parent, mv.to_vec()), -last_id);
        report.nodes_added += 1;
        Ok(-last_id)
    };
    // The other game's nodes, as this one's.
    let mut map: HashMap<i64, i64> = HashMap::from([(0, 0)]);
    for (i, mv) in s_main.iter().enumerate() {
        let at = place(map[&(i as i64)], mv, fallback)?;
        map.insert(i as i64 + 1, at);
    }
    for (id, parent, mv, origin) in &s_rows {
        if let Some(&p) = map.get(parent) {
            let at = place(p, mv, *origin)?;
            map.insert(-id, at);
        }
    }

    // Annotations, onto the same nodes.
    type Note = (i64, String, String, i64, i64, i64, Vec<u8>);
    let notes: Vec<Note> = {
        let mut q = tx.prepare(&format!(
            "SELECT node, kind, author,
               coalesce((SELECT dst FROM temp.omap WHERE src = a.origin), {fallback}),
               created_at, modified_at, payload
             FROM src.annotations a WHERE game = ?1"
        ))?;
        q.query_map([s], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?))
        })?
        .collect::<rusqlite::Result<_>>()?
    };
    let mut find = tx.prepare(
        "SELECT origin, modified_at, payload FROM main.annotations
         WHERE game = ?1 AND node = ?2 AND kind = ?3 AND author = ?4",
    )?;
    let mut put = tx.prepare(
        "INSERT INTO main.annotations (game, node, kind, author, origin, created_at, modified_at, payload)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT (game, node, kind, author) DO UPDATE SET
           origin = excluded.origin, modified_at = excluded.modified_at, payload = excluded.payload",
    )?;
    for (node, kind, author, origin, created, modified, payload) in notes {
        let Some(&at) = map.get(&node) else { continue };
        // A game's move times follow its main line.
        if kind == "times" && !same_line {
            continue;
        }
        let source_kind = SOURCE_KINDS.contains(&kind.as_str());
        let mut author = author;
        let mut moved = false;
        loop {
            let found: Option<(i64, i64, Vec<u8>)> = find
                .query_row(params![d, at, kind, author], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                .optional()?;
            match found {
                None => {
                    put.execute(params![d, at, kind, author, origin, created, modified, payload])?;
                    if moved {
                        report.annotation_conflicts += 1;
                    } else {
                        report.annotations_added += 1;
                    }
                }
                Some((_, _, p)) if p == payload => {}
                Some(_) if source_kind => {}
                Some((o, m, _)) if o == origin => {
                    // The newer edit; on a tie (two imports in one second), the import's.
                    if modified >= m {
                        put.execute(params![d, at, kind, author, origin, created, modified, payload])?;
                        if moved {
                            report.annotation_conflicts += 1;
                        } else {
                            report.annotations_added += 1;
                        }
                    }
                }
                Some(_) => {
                    let other = conflict_author(tx, origin, &author)?;
                    if other != author {
                        author = other;
                        moved = true;
                        continue;
                    }
                }
            }
            break;
        }
    }
    Ok(true)
}

/// The header columns a game both databases have takes from the other
/// when its own are empty: what describes the game, not its moves or
/// result, which go with its tree.
const FILLED_HEADERS: [&str; 12] = [
    "event",
    "site",
    "date",
    "start_ts",
    "end_ts",
    "gold_rating",
    "silver_rating",
    "gold_rating_k",
    "silver_rating_k",
    "time_control",
    "rated",
    "postal",
];

/// The columns copied when filling in a game known by its header (all but
/// its `gid`, which it keeps).
const GAME_COLUMNS_NO_GID: &str = "source, source_id, gold_name, silver_name, gold_rating, silver_rating, \
     gold_rating_k, silver_rating_k, event, site, time_control, rated, postal, date, start_ts, end_ts, \
     winner, termination, plies, start_ply, start_board, rules, moves, fingerprint, replay_error, added_at, \
     modified_at, fetched_at";

pub(crate) fn prefixed(table: &str, columns: &str) -> String {
    columns.split(',').map(|c| format!("{table}.{}", c.trim())).collect::<Vec<_>>().join(", ")
}

pub(crate) fn hex_bytes(hex: &str) -> Option<Vec<u8>> {
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
    fn a_game_both_have_takes_the_others_headers_only_where_its_own_are_empty() {
        let dir = std::env::temp_dir().join(format!("howdah-merge-headers-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let base_path = dir.join("base.db");
        let _ = std::fs::remove_file(&base_path);
        {
            let mut base = Database::open(&base_path).unwrap();
            let mut g = arimaa("1", "Ann", "10", false);
            g.event = Some("2026 WC".into());
            g.site = Some("arimaa.com".into());
            g.start_ts = Some(1_000);
            g.time_control = Some("base".into());
            base.save(&g).unwrap();
        }
        let mut main = Database::open_in_memory().unwrap();
        // Fetched before the archive had it: no event, site or start, but
        // its own time control.
        let mut fetched = arimaa("1", "Ann", "10", false);
        (fetched.event, fetched.site, fetched.start_ts) = (None, None, None);
        fetched.time_control = Some("mine".into());
        let id = main.save(&fetched).unwrap();
        let report = main.merge_from(&base_path, "base").unwrap();
        assert_eq!((report.games_kept, report.games_completed), (1, 1));
        let g = main.load(id).unwrap();
        assert_eq!(
            (g.event.as_deref(), g.site.as_deref(), g.start_ts),
            (Some("2026 WC"), Some("arimaa.com"), Some(1_000))
        );
        assert_eq!(g.time_control.as_deref(), Some("mine"), "a header it has stays");
        let _ = std::fs::remove_dir_all(&dir);
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
            MergeReport { games_added: 1, games_filled: 1, games_kept: 1, ..Default::default() }
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
        assert_eq!(evals[0].payload, b"mine", "ours stays: newer, and neither has a depth");

        // A second merge adds nothing.
        let again = main.merge_from(&base_path, "base").unwrap();
        assert_eq!((again.games_added, again.games_filled, again.games_kept), (0, 0, 3));
        assert_eq!(again.games_completed, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn imports_records_and_joins_them_with_games_here() {
        let mut db = Database::open_in_memory().unwrap();
        let plain = record("2s ed7s\n");
        let with_id = || GameRecord::parse(&format!("[GameId \"7\"]\n{}", record("").to_record())).unwrap();
        let r = db.import_records(&[plain.clone(), with_id()], "games.txt").unwrap();
        assert_eq!(r.games_added, 2);
        // Again, with a comment and a variation: joined into the games
        // already here, matched by their moves (no ids) and by GameId.
        let commented = GameRecord::parse(&format!(
            "{}\n",
            plain.to_record().replace("2s ed7s", "2s ed7s {Hm.} (2s dh7s)")
        ))
        .unwrap();
        let r = db.import_records(&[commented], "games.txt").unwrap();
        assert_eq!((r.games_added, r.games_kept, r.nodes_added, r.annotations_added), (0, 1, 1, 1));
        assert_eq!(db.game_count().unwrap(), 2);
        assert_eq!(db.import_records(&[with_id()], "again").unwrap().games_kept, 1);
    }

    /// The game (GameId 7) as an analysis by `who` with `lines`.
    fn analysis(who: &str, lines: &str) -> GameRecord {
        let text = record("2s ed7s\n3g Ed3n\n").to_record();
        GameRecord::parse(&format!(
            "[GameId \"7\"]\n[Annotator \"{who}\"]\n{}",
            text.replace("2s ed7s\n", lines)
        ))
        .unwrap()
    }

    fn game_here(lines_from: &str) -> (Database, i64) {
        let mut db = Database::open_in_memory().unwrap();
        let plain =
            GameRecord::parse(&format!("[GameId \"7\"]\n{}", record("2s ed7s\n3g Ed3n\n").to_record()))
                .unwrap();
        db.import_records(&[plain], "games.txt").unwrap();
        let r = db.import_records_with(&[analysis("Sharp", lines_from)], "a.txt", EarlierLines::Ask).unwrap();
        assert!(!r.undecided && r.earlier.is_empty(), "the first analysis has nothing earlier: {r:?}");
        assert_eq!(r.nodes_added, 1);
        let id = r.games_joined[0];
        (db, id)
    }

    fn text(db: &Database, id: i64) -> String {
        db.load(id).unwrap().to_record().to_record()
    }

    const FIRST: &str = "2s ed7s {Sharp: 51%}\n(\n2s dh7s\n)\n";
    const SECOND: &str = "2s ed7s {Sharp: 53%}\n(\n2s hb7s\n)\n";

    #[test]
    fn a_newer_analysis_asks_then_replaces_the_earlier_lines() {
        let (mut db, id) = game_here(FIRST);
        let before = text(&db, id);
        assert!(before.contains("2s dh7s"), "{before}");
        let r = db.import_records_with(&[analysis("Sharp", SECOND)], "b.txt", EarlierLines::Ask).unwrap();
        assert!(r.undecided);
        assert_eq!(r.earlier, vec![Earlier { game: id, source: "Sharp".into(), moves: 1 }]);
        assert_eq!(text(&db, id), before, "asking changes nothing");
        let r = db.import_records_with(&[analysis("Sharp", SECOND)], "b.txt", EarlierLines::Replace).unwrap();
        assert_eq!((r.lines_removed, r.nodes_added, r.undecided), (1, 1, false));
        let after = text(&db, id);
        assert!(after.contains("2s hb7s") && !after.contains("2s dh7s"), "{after}");
        assert!(after.contains("Sharp: 53%") && !after.contains("Sharp: 51%"), "the newer comment: {after}");
        // Another engine's analysis is another source: nothing earlier.
        let r = db.import_records_with(&[analysis("OpFor", FIRST)], "c.txt", EarlierLines::Ask).unwrap();
        assert!(!r.undecided && r.earlier.is_empty(), "{r:?}");
    }

    #[test]
    fn keeping_both_keeps_the_earlier_lines() {
        let (mut db, id) = game_here(FIRST);
        let r = db.import_records_with(&[analysis("Sharp", SECOND)], "b.txt", EarlierLines::Keep).unwrap();
        assert_eq!((r.lines_removed, r.nodes_added, r.earlier.len()), (0, 1, 1));
        let after = text(&db, id);
        assert!(after.contains("2s hb7s") && after.contains("2s dh7s"), "{after}");
    }

    #[test]
    fn a_line_the_user_extended_stays_through_saves() {
        let (mut db, id) = game_here(FIRST);
        // The user goes on from Sharp's line on a board, which saves the
        // game (renumbering its variations).
        let mut g = db.load(id).unwrap();
        let tree = g.tree.as_mut().unwrap();
        let after_2g = tree.main_line()[3];
        let dh7s =
            *tree[after_2g].children().iter().find(|&&c| tree[c].mv().unwrap().notation() == "dh7s").unwrap();
        tree.add_notation(dh7s, "Ed3n").unwrap();
        db.save(&g).unwrap();
        // A plain save keeps the origins, so a later analysis still finds
        // the line, but leaves it: it's the user's now too.
        let r = db.import_records_with(&[analysis("Sharp", SECOND)], "b.txt", EarlierLines::Replace).unwrap();
        assert_eq!(r.lines_removed, 0);
        assert!(r.earlier.is_empty(), "nothing it could remove: {r:?}");
        assert!(text(&db, id).contains("2s dh7s"));
    }

    #[test]
    fn saving_keeps_where_lines_came_from() {
        let (mut db, id) = game_here(FIRST);
        let g = db.load(id).unwrap();
        db.save(&g).unwrap();
        let r = db.import_records_with(&[analysis("Sharp", SECOND)], "b.txt", EarlierLines::Ask).unwrap();
        assert_eq!(r.earlier.len(), 1, "still Sharp's after a save");
    }

    #[test]
    fn a_save_that_renumbers_lines_keeps_their_comments_origins() {
        let first = "2s ed7s {Sharp: 51%}\n(\n{Sharp: gold 49%}\n2s dh7s {[%win 49.0]}\n)\n";
        let (mut db, id) = game_here(first);
        // The user adds a line before Sharp's, so Sharp's is numbered afresh.
        let mut g = db.load(id).unwrap();
        let tree = g.tree.as_mut().unwrap();
        let before_2g = tree.main_line()[2];
        tree.add_notation(before_2g, "Hb2n").unwrap();
        db.save(&g).unwrap();
        let r = db.import_records_with(&[analysis("Sharp", SECOND)], "b.txt", EarlierLines::Replace).unwrap();
        assert_eq!(r.lines_removed, 1, "Sharp's line, comments and all, is still Sharp's: {r:?}");
        let after = text(&db, id);
        assert!(!after.contains("2s dh7s") && after.contains("2g Hb2n"), "{after}");
    }

    #[test]
    fn shared_files_carry_where_lines_came_from() {
        let (db, id) = game_here(FIRST);
        let dir = std::env::temp_dir().join(format!("howdah-merge-lines-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("shared.db");
        let options = crate::ExportOptions {
            games: crate::Selection::Games(vec![id]),
            variations: true,
            annotations: true,
            ..Default::default()
        };
        db.export(&file, &options).unwrap();
        let mut other = Database::open_in_memory().unwrap();
        other.import_file(&file, "shared.db").unwrap();
        let r = other.import_records_with(&[analysis("Sharp", SECOND)], "b.txt", EarlierLines::Ask).unwrap();
        assert_eq!(r.earlier.len(), 1, "Sharp's line is still Sharp's there: {r:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn removing_a_source_removes_its_lines() {
        let (mut db, id) = game_here(FIRST);
        let clew = db.origins().unwrap().into_iter().find(|o| o.name == "Sharp").unwrap();
        let r = db.remove_origin(clew.id).unwrap();
        assert_eq!(r.moves, 1);
        assert!(!text(&db, id).contains("2s dh7s"));
    }
}
