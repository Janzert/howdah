//! The base database: arimaa.com's game archive, built into a database
//! file, slimmed for download (no indexes or fingerprints, which the
//! merge doesn't need), and compressed with zstd, with deltas that bring
//! a database up to the newest games. A manifest beside them
//! (`base.json`, [`Manifest`]) lists them and says how to check them.
//! The app works out which it needs ([`plan`]), downloads them, checks
//! them, unpacks them and merges them in ([`Database::merge_from`]).
//!
//! Each publish ([`publish`]) imports the whole archive afresh, and the
//! archive only grows, so the publisher keeps one table, `seen.db`: each
//! game's `gid` and when it was first published. A delta *since* a time
//! holds every game first published after it, so it brings any database
//! at that time or later up to the newest; the games it repeats are
//! skipped by the merge. Each publish rebuilds the deltas, all ending at
//! the newest games and starting 1, 2, 4, 8, … publishes back, and at
//! the base, until one grows past a share of the base's size. A client
//! remembers the time it reached and takes the delta starting closest
//! before it, or the base and the delta after it: two files at most.

use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{Database, Error, Result, schema};

/// The manifest's format.
pub const MANIFEST_FORMAT: u32 = 1;

/// zstd's window for the packed files: 128 MB, which unpacking must allow.
const WINDOW_LOG: u32 = 27;

/// The publisher's record of when each game was first published.
pub const SEEN_DB: &str = "seen.db";
pub const MANIFEST_FILE: &str = "base.json";

/// What a release holds: the base and the deltas to the newest games.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub format: u32,
    /// When the newest games were first published (Unix seconds): a
    /// database that has reached this has them all.
    pub at: i64,
    /// The games there are in all, and the newest one's date
    /// (`YYYY.MM.DD`).
    pub games: i64,
    pub through: Option<String>,
    pub base: Pack,
    /// Each holds the games first published after its `since`, up to `at`.
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
    /// The schema version it was built with.
    pub schema: i64,
    /// The games in the file.
    pub games: i64,
    /// For a delta, the time after which its games were first published.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<i64>,
    /// The time a database reaches with it.
    pub at: i64,
}

impl Manifest {
    /// Checks that the deltas end at the newest games and that the base
    /// can be brought up to them.
    pub fn check(&self) -> Result<()> {
        let bad = |why: &str| Err(Error::Archive(format!("the manifest {why}")));
        if self.base.since.is_some() {
            return bad("has a base that isn't one");
        }
        if self.deltas.iter().any(|d| d.at != self.at || !d.since.is_some_and(|s| s < self.at)) {
            return bad("has a delta that doesn't end at the newest games");
        }
        if self.base.at < self.at && self.delta_for(self.base.at).is_none() {
            return bad("has no delta after its base");
        }
        Ok(())
    }

    /// The delta that brings a database at `t` to the newest: the one
    /// starting at or closest before it.
    fn delta_for(&self, t: i64) -> Option<&Pack> {
        self.deltas.iter().filter(|d| d.since.is_some_and(|s| s <= t)).max_by_key(|d| d.since)
    }
}

/// The packs a database at `at` (the time it reached, or `None`) needs
/// to have the manifest's newest games: none when it's there, else the
/// delta starting closest before it, else the base and the delta after
/// that.
pub fn plan(manifest: &Manifest, at: Option<i64>) -> Result<Vec<&Pack>> {
    manifest.check()?;
    if let Some(t) = at {
        if t >= manifest.at {
            return Ok(Vec::new());
        }
        if let Some(d) = manifest.delta_for(t) {
            return Ok(vec![d]);
        }
    }
    let mut packs = vec![&manifest.base];
    if manifest.base.at < manifest.at {
        packs.extend(manifest.delta_for(manifest.base.at));
    }
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

    /// Keeps only the games `seen` (a [`SEEN_DB`]) first saw after
    /// `since`, and the players they name: a delta. Run on a copy of the
    /// newest import, then [`Database::vacuum`].
    fn keep_seen_after(&mut self, seen: &Path, since: i64) -> Result<()> {
        let path = seen.to_str().ok_or_else(|| Error::Archive("the seen table's path isn't text".into()))?;
        let conn = self.writable()?;
        conn.execute("ATTACH DATABASE ?1 AS seen", [path])?;
        let result = (|| {
            let tx = conn.transaction()?;
            tx.execute(
                "DELETE FROM main.games WHERE gid NOT IN (SELECT gid FROM seen.seen WHERE first > ?1)",
                [since],
            )?;
            tx.execute(
                "DELETE FROM main.players WHERE id NOT IN
                   (SELECT gold FROM main.games WHERE gold IS NOT NULL
                    UNION SELECT silver FROM main.games WHERE silver IS NOT NULL)",
                [],
            )?;
            tx.execute("INSERT OR REPLACE INTO meta (key, value) VALUES ('kind', 'delta')", [])?;
            tx.commit()
        })();
        conn.execute("DETACH DATABASE seen", [])?;
        Ok(result?)
    }
}

/// Opens the publisher's seen table, making it if there's none.
fn open_seen(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS seen (gid BLOB PRIMARY KEY, first INTEGER NOT NULL) WITHOUT ROWID;
         CREATE INDEX IF NOT EXISTS seen_first ON seen (first);",
    )?;
    Ok(conn)
}

/// What [`publish`] is to do.
#[derive(Clone, Debug)]
pub struct PublishOptions {
    /// This publish's time (Unix seconds), after every earlier one.
    pub at: i64,
    /// Pack a new base even when there's one.
    pub new_base: bool,
    /// Stop adding older deltas once one is larger than this share of
    /// the base (percent).
    pub cap_percent: u64,
    pub threads: u32,
}

/// What a publish did.
#[derive(Clone, Debug)]
pub struct Published {
    pub manifest: Manifest,
    /// Games first published now.
    pub new_games: i64,
    /// Files the earlier manifest named that this one doesn't: to take
    /// out of the release once the new manifest is up.
    pub dropped: Vec<String>,
}

/// Publishes `newest`, a fresh import of the whole archive (stripped and
/// vacuumed), into `out_dir`: records its new games in [`SEEN_DB`],
/// packs a base if asked or if there's none, rebuilds the deltas, and
/// writes the manifest. `None` when there's nothing new to publish.
pub fn publish(out_dir: &Path, newest: &Path, opts: &PublishOptions) -> Result<Option<Published>> {
    let seen_path = out_dir.join(SEEN_DB);
    let manifest_path = out_dir.join(MANIFEST_FILE);
    let old: Option<Manifest> = match std::fs::read(&manifest_path) {
        Ok(bytes) => Some(serde_json::from_slice(&bytes).map_err(|e| Error::Archive(e.to_string()))?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.into()),
    };
    let new_games = {
        let path = newest.to_str().ok_or_else(|| Error::Archive("the import's path isn't text".into()))?;
        let mut seen = open_seen(&seen_path)?;
        let last: Option<i64> = seen.query_row("SELECT max(first) FROM seen", [], |r| r.get(0))?;
        if last.is_some_and(|l| opts.at <= l) || old.as_ref().is_some_and(|m| opts.at <= m.base.at) {
            return Err(Error::Archive("this publish's time isn't after the last one's".into()));
        }
        seen.execute("ATTACH DATABASE ?1 AS newest", [path])?;
        let tx = seen.transaction()?;
        let n = tx
            .execute("INSERT OR IGNORE INTO seen (gid, first) SELECT gid, ?1 FROM newest.games", [opts.at])?;
        if n == 0 && !opts.new_base && old.is_some() {
            return Ok(None); // the transaction rolls back
        }
        tx.commit()?;
        seen.execute("DETACH DATABASE newest", [])?;
        n as i64
    };
    let seen = open_seen(&seen_path)?;
    let at: i64 = seen.query_row("SELECT max(first) FROM seen", [], |r| r.get(0))?;
    let times: Vec<i64> = {
        let mut q = seen.prepare("SELECT DISTINCT first FROM seen WHERE first < ?1 ORDER BY first DESC")?;
        q.query_map([at], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?
    };
    drop(seen);

    let base = match &old {
        Some(m) if !opts.new_base => m.base.clone(),
        _ => pack(
            newest,
            &out_dir.join(format!("howdah-games-{}.db.zst", stamp(opts.at))),
            opts.threads,
            None,
            opts.at,
        )?,
    };

    // Deltas from 1, 2, 4, 8, … publishes back, and from the base.
    let mut starts: Vec<i64> =
        (0..).map(|i| (1usize << i) - 1).take_while(|&k| k < times.len()).map(|k| times[k]).collect();
    if base.at < at && !starts.contains(&base.at) {
        starts.push(base.at);
    }
    starts.sort_unstable_by(|a, b| b.cmp(a));
    let cap = base.size.saturating_mul(opts.cap_percent) / 100;
    let mut deltas = Vec::new();
    let work = out_dir.join("delta-work.db");
    for since in starts {
        let _ = std::fs::remove_file(&work);
        std::fs::copy(newest, &work)?;
        {
            let mut d = Database::open(&work)?;
            d.keep_seen_after(&seen_path, since)?;
            d.vacuum()?;
        }
        let file = out_dir.join(format!("howdah-games-{}-since-{}.db.zst", stamp(opts.at), stamp(since)));
        let p = pack(&work, &file, opts.threads, Some(since), at)?;
        let needed = since == base.at;
        if p.size > cap && !needed {
            std::fs::remove_file(&file)?;
            if since < base.at {
                break;
            }
            continue;
        }
        deltas.push(p);
    }
    let _ = std::fs::remove_file(&work);

    let (games, through) = {
        let d = Database::open_read_only(newest)?;
        (d.game_count()?, d.newest_date()?)
    };
    let manifest = Manifest { format: MANIFEST_FORMAT, at, games, through, base, deltas };
    manifest.check()?;
    let tmp = out_dir.join(format!("{MANIFEST_FILE}.new"));
    std::fs::write(
        &tmp,
        serde_json::to_string_pretty(&manifest).map_err(|e| Error::Archive(e.to_string()))? + "\n",
    )?;
    std::fs::rename(tmp, &manifest_path)?;
    let named = |m: &Manifest| -> Vec<String> {
        std::iter::once(&m.base).chain(&m.deltas).map(|p| p.file.clone()).collect()
    };
    let now_named = named(&manifest);
    let dropped =
        old.map(|m| named(&m)).unwrap_or_default().into_iter().filter(|f| !now_named.contains(f)).collect();
    Ok(Some(Published { manifest, new_games, dropped }))
}

/// `20261118-020715` (UTC) for Unix seconds, for file names.
fn stamp(ts: i64) -> String {
    let secs = ts.rem_euclid(86_400);
    let date = crate::game::utc_date(ts).replace('.', "");
    format!("{date}-{:02}{:02}{:02}", secs / 3600, secs / 60 % 60, secs % 60)
}

/// Compresses `db` into `out` and returns its pack. A file already at
/// `out` is left alone, since a published manifest may name it.
pub fn pack(db: &Path, out: &Path, threads: u32, since: Option<i64>, at: i64) -> Result<Pack> {
    if out.exists() {
        return Err(Error::Archive(format!("{} is there already", out.display())));
    }
    let games = Database::open_read_only(db)?.game_count()?;
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
        schema: schema::VERSION,
        games,
        since,
        at,
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

    fn game(id: u32) -> GameData {
        let mut g = GameData::new();
        g.date = Some(format!("2026.{:02}.{:02}", 1 + id / 28, 1 + id % 28));
        let key = crate::SourceKey::arimaa_com(id.to_string());
        g.gid = crate::gid_for(&key);
        g.source = Some(key);
        // Each game brings a player of its own and shares one.
        g.players[0].name = Some("regular".into());
        g.players[0].site_id = Some("regular".into());
        g.players[1].name = Some(format!("p{id}"));
        g.players[1].site_id = Some(format!("p{id}"));
        g
    }

    /// An import of the archive with games `1..=n`.
    fn import(path: &Path, n: u32) {
        let _ = std::fs::remove_file(path);
        let mut d = Database::open(path).unwrap();
        for id in 1..=n {
            d.save_as(&game(id), SaveMode::Source).unwrap();
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
        import(&db, 1);
        let packed = dir.join("base.db.zst");
        let p = pack(&db, &packed, 2, None, 100).unwrap();
        assert_eq!((p.games, p.at, p.schema, p.since), (1, 100, schema::VERSION, None));
        check(&packed, &p).unwrap();
        let out = dir.join("unpacked.db");
        unpack(&packed, &out, p.unpacked_size).unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), std::fs::read(&db).unwrap());
        assert!(unpack(&packed, &out, p.unpacked_size - 1).is_err());
        let mut wrong = p.clone();
        wrong.sha256 = "00".into();
        assert!(check(&packed, &wrong).is_err());
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("\"unpackedSize\"") && !json.contains("\"since\""));
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn opts(at: i64) -> PublishOptions {
        PublishOptions { at, new_base: false, cap_percent: 100_000, threads: 2 }
    }

    fn starts(m: &Manifest) -> Vec<i64> {
        m.deltas.iter().map(|d| d.since.unwrap()).collect()
    }

    /// Installs what the plan says into `user`, as the app does, and
    /// returns the time reached.
    fn update(dir: &Path, user: &mut Database, at: Option<i64>) -> Option<i64> {
        let m: Manifest = serde_json::from_slice(&std::fs::read(dir.join(MANIFEST_FILE)).unwrap()).unwrap();
        let mut reached = at;
        for p in plan(&m, at).unwrap() {
            let out = dir.join("unpacked.db");
            unpack(&dir.join(&p.file), &out, p.unpacked_size).unwrap();
            user.merge_from(&out, "base").unwrap();
            reached = Some(p.at);
        }
        reached
    }

    #[test]
    fn publishes_back_deltas_that_bring_any_database_up() {
        let dir = temp_dir("publish");
        let newest = dir.join("newest.db");
        // Week 1: the base, 10 games.
        import(&newest, 10);
        let p = publish(&dir, &newest, &opts(1000)).unwrap().unwrap();
        assert_eq!((p.new_games, p.manifest.at, p.manifest.base.games), (10, 1000, 10));
        assert!(p.manifest.deltas.is_empty());
        let mut early = Database::open_in_memory().unwrap();
        let early_at = update(&dir, &mut early, None);
        assert_eq!((early_at, early.game_count().unwrap()), (Some(1000), 10));

        // Weeks 2 to 6, two games each; nothing new in week 7.
        let mut mid = None;
        for week in 2..=6 {
            import(&newest, 10 + 2 * (week - 1));
            let p = publish(&dir, &newest, &opts(1000 * week as i64)).unwrap().unwrap();
            assert_eq!(p.new_games, 2);
            if week == 3 {
                let mut db = Database::open_in_memory().unwrap();
                mid = Some((update(&dir, &mut db, None), db));
            }
        }
        assert!(publish(&dir, &newest, &opts(7000)).unwrap().is_none());
        let m: Manifest = serde_json::from_slice(&std::fs::read(dir.join(MANIFEST_FILE)).unwrap()).unwrap();
        assert_eq!((m.at, m.games), (6000, 20));
        // 1, 2 and 4 publishes back, and the base.
        assert_eq!(starts(&m), [5000, 4000, 2000, 1000]);
        assert!(m.deltas.iter().all(|d| d.at == 6000));
        assert_eq!(m.deltas.iter().map(|d| d.games).collect::<Vec<_>>(), [2, 4, 8, 10]);

        // The base's database takes the delta from the base; week 3's the
        // one since week 2 (it repeats week 3's games); a new one gets
        // the base and the delta after it.
        let files = |at| plan(&m, at).unwrap().iter().map(|p| p.since).collect::<Vec<_>>();
        assert_eq!(files(Some(1000)), [Some(1000)]);
        assert_eq!(files(Some(3000)), [Some(2000)]);
        assert_eq!(files(Some(5500)), [Some(5000)]);
        assert!(files(Some(6000)).is_empty());
        assert_eq!(files(None), [None, Some(1000)]);
        assert_eq!(files(Some(10)), [None, Some(1000)]);
        assert_eq!(update(&dir, &mut early, early_at), Some(6000));
        assert_eq!(early.game_count().unwrap(), 20);
        let (mid_at, mut mid_db) = mid.unwrap();
        assert_eq!(mid_at, Some(3000));
        assert_eq!(update(&dir, &mut mid_db, mid_at), Some(6000));
        assert_eq!(mid_db.game_count().unwrap(), 20);
        // The newest delta has its two games and only their players.
        let out = dir.join("unpacked.db");
        unpack(&dir.join(&m.deltas[0].file), &out, m.deltas[0].unpacked_size).unwrap();
        let players: i64 = Database::open_read_only(&out)
            .unwrap()
            .connection()
            .query_row("SELECT count(*) FROM players", [], |r| r.get(0))
            .unwrap();
        assert_eq!(players, 3);

        // A new base in week 8: databases from before it still get a
        // delta, and the earlier files are listed to take down.
        import(&newest, 22);
        let p = publish(&dir, &newest, &PublishOptions { new_base: true, ..opts(8000) }).unwrap().unwrap();
        assert_eq!((p.manifest.base.at, p.manifest.base.games, p.manifest.at), (8000, 22, 8000));
        assert_eq!(starts(&p.manifest), [6000, 5000, 3000]);
        assert_eq!(plan(&p.manifest, Some(5500)).unwrap()[0].since, Some(5000));
        assert_eq!(plan(&p.manifest, None).unwrap().len(), 1);
        assert_eq!(p.dropped.len(), 5);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn older_deltas_stop_at_the_cap() {
        let dir = temp_dir("cap");
        let newest = dir.join("newest.db");
        import(&newest, 200);
        publish(&dir, &newest, &opts(1000)).unwrap();
        for week in 2..=5 {
            import(&newest, 200 + 40 * (week - 1));
            publish(&dir, &newest, &PublishOptions { cap_percent: 40, ..opts(1000 * week as i64) }).unwrap();
        }
        let m: Manifest = serde_json::from_slice(&std::fs::read(dir.join(MANIFEST_FILE)).unwrap()).unwrap();
        // The delta from 1 publish back fits, 2 back doesn't; the base's
        // is kept however large, since a new database needs it.
        // (Small databases are mostly overhead: about 33% of the base for
        // one publish back, 49% for two.)
        assert_eq!(starts(&m), [4000, 1000]);
        assert!(m.deltas[0].size <= m.base.size * 40 / 100);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_broken_manifest_is_refused() {
        let pack = |since, at| Pack {
            file: "f".into(),
            size: 1,
            sha256: String::new(),
            unpacked_size: 1,
            schema: 1,
            games: 1,
            since,
            at,
        };
        let mut m = Manifest {
            format: MANIFEST_FORMAT,
            at: 20,
            games: 2,
            through: None,
            base: pack(None, 10),
            deltas: vec![pack(Some(15), 20)],
        };
        assert!(plan(&m, None).is_err(), "nothing after the base");
        m.deltas[0].since = Some(10);
        assert_eq!(plan(&m, None).unwrap().len(), 2);
        m.deltas[0].at = 19;
        assert!(plan(&m, Some(12)).is_err(), "a delta short of the newest");
    }
}
