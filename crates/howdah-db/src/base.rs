//! The base database: arimaa.com's game archive, built into a database
//! file, slimmed for download (no indexes or fingerprints, which the
//! merge doesn't need), and compressed with zstd. A manifest beside it
//! (`base.json`, [`Manifest`]) lists the full base and the deltas after
//! it, each a [`Pack`], and says how to check them. The app works out
//! which files its database needs ([`plan`]), downloads them, checks
//! them, unpacks them and merges them in ([`Database::merge_from`]).
//!
//! Each pack leaves the database in a *state*, named by a uuid: the full
//! base's own, or for a delta the uuid of the database it was taken from
//! (the newer of the two it compares). A delta also names the state it
//! applies to (`from`), so the deltas form a chain, and an app that
//! remembers the last state it reached needs only the deltas after it.
//! Merging skips games the database has, so a pack applied over a state
//! that already has some of its games does no harm.

use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{Database, Error, Result, now, schema};

/// The manifest's format.
pub const MANIFEST_FORMAT: u32 = 1;

/// zstd's window for the packed files: 128 MB, which unpacking must allow.
const WINDOW_LOG: u32 = 27;

/// What a base database release holds: the full base, and the deltas
/// that bring a database on from it (or from earlier states) to the
/// newest. Each delta's `from` is the `uuid` of the one before it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub format: u32,
    pub base: Pack,
    #[serde(default)]
    pub deltas: Vec<Pack>,
}

/// One packed database file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pack {
    /// The compressed file's name, beside the manifest.
    pub file: String,
    /// Its size and SHA-256 (hex).
    pub size: u64,
    pub sha256: String,
    /// The unpacked database's size, to check there's room.
    pub unpacked_size: u64,
    /// The state the database is in after this pack.
    pub uuid: String,
    /// For a delta, the state it applies to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    /// The schema version it was built with.
    pub schema: i64,
    /// The games in the file.
    pub games: i64,
    /// The games in the state it leaves (for the base, `games`).
    pub total: i64,
    /// The newest game's date in that state (`YYYY.MM.DD`).
    pub through: Option<String>,
    /// When it was built (Unix seconds).
    pub built: i64,
}

impl Manifest {
    pub fn new(base: Pack) -> Manifest {
        Manifest { format: MANIFEST_FORMAT, base, deltas: Vec::new() }
    }

    /// The newest pack: the last delta, else the base.
    pub fn newest(&self) -> &Pack {
        self.deltas.last().unwrap_or(&self.base)
    }

    /// Checks that the deltas form one chain and that the base is on it.
    pub fn check_chain(&self) -> Result<()> {
        let bad = |why: &str| Err(Error::Archive(format!("the manifest's deltas {why}")));
        for pair in self.deltas.windows(2) {
            if pair[1].from.as_deref() != Some(pair[0].uuid.as_str()) {
                return bad("don't follow on from each other");
            }
        }
        if let Some(first) = self.deltas.first()
            && first.from.is_none()
        {
            return bad("don't say what they apply to");
        }
        if !self.deltas.is_empty() && self.after(&self.base.uuid).is_none() {
            return bad("don't pass through the base");
        }
        Ok(())
    }

    /// The deltas after `state`, when it's on the chain.
    fn after(&self, state: &str) -> Option<&[Pack]> {
        if self.newest().uuid == state {
            return Some(&[]);
        }
        let i = self.deltas.iter().position(|d| d.from.as_deref() == Some(state))?;
        Some(&self.deltas[i..])
    }
}

/// The packs a database in `state` (the last one it reached, or `None`)
/// needs to reach the manifest's newest: the deltas after it when it's
/// on the chain, else the base and the deltas after that. Empty when it's
/// there already.
pub fn plan<'a>(manifest: &'a Manifest, state: Option<&str>) -> Result<Vec<&'a Pack>> {
    manifest.check_chain()?;
    if let Some(rest) = state.and_then(|s| manifest.after(s)) {
        return Ok(rest.iter().collect());
    }
    let mut packs = vec![&manifest.base];
    packs.extend(manifest.after(&manifest.base.uuid).unwrap_or(&[]));
    Ok(packs)
}

impl Database {
    /// Makes this database a base database: fingerprints cleared, indexes
    /// dropped (the unique keys stay, as constraints), and the facts the
    /// manifest gives kept in `meta`. Run on a fresh import, then
    /// [`Database::vacuum`].
    pub fn strip_for_base(&mut self) -> Result<()> {
        let conn = self.writable()?;
        let tx = conn.transaction()?;
        tx.execute("UPDATE games SET fingerprint = NULL", [])?;
        let indexes: Vec<String> = {
            let mut q =
                tx.prepare("SELECT name FROM sqlite_master WHERE type = 'index' AND sql IS NOT NULL")?;
            q.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?
        };
        for name in indexes {
            tx.execute(&format!("DROP INDEX \"{name}\""), [])?;
        }
        tx.execute("INSERT OR REPLACE INTO meta (key, value) VALUES ('kind', 'base')", [])?;
        tx.commit()?;
        Ok(())
    }

    pub fn vacuum(&mut self) -> Result<()> {
        self.writable()?.execute_batch("PRAGMA journal_mode = DELETE; VACUUM;")?;
        Ok(())
    }

    /// The newest game's date.
    pub fn newest_date(&self) -> Result<Option<String>> {
        Ok(self.connection().query_row("SELECT max(date) FROM games", [], |r| r.get(0))?)
    }

    /// Takes out every game `older` has (by `gid`), and the players no
    /// game left names, leaving what's new: a delta. Run on a copy of
    /// the newer base, then [`Database::vacuum`].
    pub fn keep_only_new(&mut self, older: &Path) -> Result<()> {
        let path = older.to_str().ok_or_else(|| Error::Archive("the older base's path isn't text".into()))?;
        let conn = self.writable()?;
        conn.execute("ATTACH DATABASE ?1 AS older", [path])?;
        let result = (|| {
            let tx = conn.transaction()?;
            tx.execute("DELETE FROM main.games WHERE gid IN (SELECT gid FROM older.games)", [])?;
            tx.execute(
                "DELETE FROM main.players WHERE id NOT IN
                   (SELECT gold FROM main.games WHERE gold IS NOT NULL
                    UNION SELECT silver FROM main.games WHERE silver IS NOT NULL)",
                [],
            )?;
            tx.execute("INSERT OR REPLACE INTO meta (key, value) VALUES ('kind', 'delta')", [])?;
            tx.commit()
        })();
        conn.execute("DETACH DATABASE older", [])?;
        Ok(result?)
    }
}

/// Builds a delta from two base databases (each stripped and vacuumed):
/// the games in `newer` that `older` lacks, in an unpacked database at
/// `out`. Its state is `newer`'s.
pub fn make_delta(older: &Path, newer: &Path, out: &Path) -> Result<()> {
    std::fs::copy(newer, out)?;
    let mut d = Database::open(out)?;
    d.keep_only_new(older)?;
    d.vacuum()
}

/// Compresses `db` into `out` and returns its pack. For a delta, `from`
/// is the state it applies to and `state` the database it was taken
/// from, which gives the total and the newest date; for a base, both are
/// `None`.
pub fn pack(db: &Path, out: &Path, threads: u32, from: Option<String>, state: Option<&Path>) -> Result<Pack> {
    let (uuid, games, mut total, mut through) = {
        let d = Database::open_read_only(db)?;
        let games = d.game_count()?;
        (d.uuid()?, games, games, d.newest_date()?)
    };
    if let Some(state) = state {
        let s = Database::open_read_only(state)?;
        total = s.game_count()?;
        through = s.newest_date()?;
    }
    let unpacked_size = std::fs::metadata(db)?.len();
    {
        let mut input = BufReader::new(File::open(db)?);
        let mut enc = zstd::Encoder::new(BufWriter::new(File::create(out)?), 22)?;
        enc.long_distance_matching(true)?;
        enc.window_log(WINDOW_LOG)?;
        enc.multithread(threads.clamp(1, 8))?;
        std::io::copy(&mut input, &mut enc)?;
        enc.finish()?.flush()?;
    }
    let (size, sha256) = digest(out)?;
    Ok(Pack {
        file: out.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string(),
        size,
        sha256,
        unpacked_size,
        uuid,
        from,
        schema: schema::VERSION,
        games,
        total,
        through,
        built: now(),
    })
}

/// A file's size and SHA-256 (hex).
pub fn digest(path: &Path) -> Result<(u64, String)> {
    let mut f = File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    let mut size = 0u64;
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        size += n as u64;
        h.update(&buf[..n]);
    }
    Ok((size, crate::hex(&h.finalize())))
}

/// Checks a downloaded file against its pack.
pub fn check(path: &Path, pack: &Pack) -> Result<()> {
    let (size, sha) = digest(path)?;
    if size != pack.size || !sha.eq_ignore_ascii_case(&pack.sha256) {
        return Err(Error::Archive(format!(
            "the download doesn't match its manifest ({size} bytes, SHA-256 {sha})"
        )));
    }
    Ok(())
}

/// Unpacks a packed file into `out`, stopping past `limit` bytes.
pub fn unpack(path: &Path, out: &Path, limit: u64) -> Result<()> {
    let mut dec = zstd::Decoder::new(File::open(path)?)?;
    dec.window_log_max(WINDOW_LOG)?;
    let mut w = BufWriter::new(File::create(out)?);
    let copied = std::io::copy(&mut dec.take(limit + 1), &mut w)?;
    w.flush()?;
    if copied > limit {
        return Err(Error::Archive("the database unpacks larger than its manifest says".into()));
    }
    Ok(())
}

/// Where to put a file being built next to `out`.
pub fn work_path(out: &Path, suffix: &str) -> PathBuf {
    let mut name = out.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(suffix);
    out.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GameData, SaveMode};

    fn game(id: &str, date: &str) -> GameData {
        let mut g = GameData::new();
        g.date = Some(date.into());
        let key = crate::SourceKey::arimaa_com(id);
        g.gid = crate::gid_for(&key);
        g.source = Some(key);
        // Game 3 brings a new player; the others share theirs.
        for (i, p) in g.players.iter_mut().enumerate() {
            let name = if id == "3" && i == 1 { "newcomer".to_string() } else { format!("p{i}") };
            p.site_id = Some(name.clone());
            p.name = Some(name);
        }
        g
    }

    fn base_db(path: &Path, games: &[(&str, &str)]) {
        let _ = std::fs::remove_file(path);
        let mut d = Database::open(path).unwrap();
        for (id, date) in games {
            d.save_as(&game(id, date), SaveMode::Source).unwrap();
        }
        d.strip_for_base().unwrap();
        d.vacuum().unwrap();
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("howdah-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn packs_checks_and_unpacks() {
        let dir = temp_dir("base");
        let db = dir.join("base.db");
        base_db(&db, &[("1", "2026.10.11")]);
        let packed = dir.join("base.db.zst");
        let m = pack(&db, &packed, 2, None, None).unwrap();
        assert_eq!(
            (m.games, m.total, m.through.as_deref(), m.schema, m.from.as_deref()),
            (1, 1, Some("2026.10.11"), schema::VERSION, None)
        );
        check(&packed, &m).unwrap();
        let out = dir.join("unpacked.db");
        unpack(&packed, &out, m.unpacked_size).unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), std::fs::read(&db).unwrap());
        assert!(unpack(&packed, &out, m.unpacked_size - 1).is_err());
        let mut wrong = m.clone();
        wrong.sha256 = "00".into();
        assert!(check(&packed, &wrong).is_err());
        let json = serde_json::to_string(&Manifest::new(m)).unwrap();
        assert!(json.contains("\"unpackedSize\"") && json.contains("\"deltas\":[]"));
        assert!(!json.contains("\"from\""));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_delta_holds_only_the_new_games_and_merges_in() {
        let dir = temp_dir("delta");
        let (old, new) = (dir.join("old.db"), dir.join("new.db"));
        base_db(&old, &[("1", "2026.10.11"), ("2", "2026.10.12")]);
        base_db(&new, &[("1", "2026.10.11"), ("2", "2026.10.12"), ("3", "2026.10.19")]);
        let delta = dir.join("delta.db");
        make_delta(&old, &new, &delta).unwrap();
        let old_uuid = Database::open_read_only(&old).unwrap().uuid().unwrap();
        let p = pack(&delta, &dir.join("delta.db.zst"), 2, Some(old_uuid.clone()), Some(&new)).unwrap();
        assert_eq!((p.games, p.total, p.through.as_deref()), (1, 3, Some("2026.10.19")));
        assert_eq!(p.uuid, Database::open_read_only(&new).unwrap().uuid().unwrap());
        assert_eq!(p.from, Some(old_uuid));
        let players: i64 = Database::open_read_only(&delta)
            .unwrap()
            .connection()
            .query_row("SELECT count(*) FROM players", [], |r| r.get(0))
            .unwrap();
        assert_eq!(players, 2, "only the delta's game's players");

        let mut user = Database::open_in_memory().unwrap();
        user.merge_from(&old, "base").unwrap();
        let report = user.merge_from(&delta, "base").unwrap();
        assert_eq!((report.games_added, user.game_count().unwrap()), (1, 3));
        // Again, or over a database that has the games: nothing more.
        assert_eq!(user.merge_from(&delta, "base").unwrap().games_added, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn fake(uuid: &str, from: Option<&str>) -> Pack {
        Pack {
            file: format!("{uuid}.db.zst"),
            size: 1,
            sha256: String::new(),
            unpacked_size: 1,
            uuid: uuid.into(),
            from: from.map(Into::into),
            schema: 1,
            games: 1,
            total: 1,
            through: None,
            built: 0,
        }
    }

    fn files(packs: Vec<&Pack>) -> Vec<&str> {
        packs.iter().map(|p| p.uuid.as_str()).collect()
    }

    #[test]
    fn plans_what_a_database_needs() {
        // B is the base; a → b → B → c → d are deltas, the first two
        // leading up to the base, which was repacked from b's state.
        let mut m = Manifest::new(fake("B", None));
        assert_eq!(files(plan(&m, None).unwrap()), ["B"]);
        assert!(plan(&m, Some("B")).unwrap().is_empty());
        assert_eq!(files(plan(&m, Some("old")).unwrap()), ["B"]);
        m.deltas = vec![fake("c", Some("B")), fake("d", Some("c"))];
        assert_eq!(files(plan(&m, None).unwrap()), ["B", "c", "d"]);
        assert_eq!(files(plan(&m, Some("B")).unwrap()), ["c", "d"]);
        assert_eq!(files(plan(&m, Some("c")).unwrap()), ["d"]);
        assert!(plan(&m, Some("d")).unwrap().is_empty());
        assert_eq!(files(plan(&m, Some("old")).unwrap()), ["B", "c", "d"]);
        m.deltas.insert(0, fake("B", Some("a")));
        m.deltas.insert(0, fake("a", Some("z")));
        assert_eq!(files(plan(&m, Some("z")).unwrap()), ["a", "B", "c", "d"]);
        assert_eq!(files(plan(&m, None).unwrap()), ["B", "c", "d"]);
        // A broken chain, or one that misses the base, is refused.
        m.deltas.remove(1);
        assert!(plan(&m, None).is_err());
        let mut off = Manifest::new(fake("B", None));
        off.deltas = vec![fake("c", Some("X"))];
        assert!(plan(&off, None).is_err());
    }
}
