//! The game database's command line.
//!
//! ```text
//! howdah-db import DB PATH... [-j N]   arimaa.com archive files (or directories of them)
//! howdah-db stats DB                   counts and sizes
//! howdah-db show DB ID                 a game's record (`ID`, or `arimaa:ID` for a permanent id)
//! howdah-db list DB [PLAYER [OPPONENT]]   the newest 20 games, timed
//! howdah-db base OUTDIR PATH... [-j N]    builds the base database from archive files:
//!                                       OUTDIR/howdah-games-<date>.db.zst, OUTDIR/base.json, and
//!                                       OUTDIR/state.db (kept for the next delta)
//! howdah-db delta OUTDIR PATH... [-j N]   the games added since OUTDIR/state.db, as
//!                                       OUTDIR/howdah-games-<date>-delta.db.zst, added to base.json
//! howdah-db merge DB OTHER              merges another database file into DB
//! howdah-db verify DB                  loads every game back, reporting any that fail
//! ```

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use howdah_db::{ARIMAA_COM, Database, archive, base};

fn usage() -> ExitCode {
    eprintln!(
        "usage: howdah-db import DB PATH... [-j N] | stats DB | show DB ID|arimaa:ID | verify DB | list DB [PLAYER [OPPONENT]] | base OUTDIR PATH... | delta OUTDIR PATH... | merge DB OTHER"
    );
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("import") if args.len() >= 3 => import(&args[1], &args[2..]),
        Some("stats") if args.len() == 2 => stats(&args[1]),
        Some("show") if args.len() == 3 => show(&args[1], &args[2]),
        Some("verify") if args.len() == 2 => verify(&args[1]),
        Some("base") if args.len() >= 3 => base(&args[1], &args[2..]),
        Some("delta") if args.len() >= 3 => delta(&args[1], &args[2..]),
        Some("merge") if args.len() == 3 => merge(&args[1], &args[2]),
        Some("list") if (2..=4).contains(&args.len()) => list(&args[1], args.get(2), args.get(3)),
        _ => return usage(),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("howdah-db: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Paths and `-j N` from the arguments.
fn paths_and_threads(rest: &[String]) -> Result<(Vec<PathBuf>, usize), Box<dyn std::error::Error>> {
    let mut threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(8);
    let mut paths = Vec::new();
    let mut it = rest.iter();
    while let Some(a) = it.next() {
        if a == "-j" {
            threads = it.next().ok_or("-j needs a number")?.parse()?;
        } else {
            paths.push(PathBuf::from(a));
        }
    }
    Ok((paths, threads))
}

fn import(db: &str, rest: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let (paths, threads) = paths_and_threads(rest)?;
    let files = archive::archive_files(&paths)?;
    let mut db = Database::open(db)?;
    let start = Instant::now();
    let report = db.import_archive(&files, threads, |path, n| {
        eprintln!("{:>7.1}s {}: {n} games", start.elapsed().as_secs_f64(), path.display());
    })?;
    eprintln!(
        "{} files ({} unchanged and skipped), {} games, {} with moves that didn't replay, in {:.1}s",
        report.files,
        report.skipped_files,
        report.games,
        report.replay_errors,
        start.elapsed().as_secs_f64()
    );
    Ok(())
}

fn stats(db: &str) -> Result<(), Box<dyn std::error::Error>> {
    let db = Database::open_read_only(db)?;
    let c = db.connection();
    let one = |sql: &str| -> i64 { c.query_row(sql, [], |r| r.get(0)).unwrap_or(0) };
    println!("games: {}", one("SELECT count(*) FROM games"));
    println!("  with moves: {}", one("SELECT count(*) FROM games WHERE moves IS NOT NULL"));
    println!("  replay errors: {}", one("SELECT count(*) FROM games WHERE replay_error IS NOT NULL"));
    println!("plies: {}", one("SELECT coalesce(sum(plies), 0) FROM games"));
    println!("players: {}", one("SELECT count(*) FROM players"));
    println!("variation nodes: {}", one("SELECT count(*) FROM nodes"));
    println!("annotations: {}", one("SELECT count(*) FROM annotations"));
    println!("move bytes: {}", one("SELECT coalesce(sum(length(moves)), 0) FROM games"));
    println!(
        "time bytes: {}",
        one("SELECT coalesce(sum(length(payload)), 0) FROM annotations WHERE kind = 'times'")
    );
    let pages: i64 = c.query_row("PRAGMA page_count", [], |r| r.get(0))?;
    let size: i64 = c.query_row("PRAGMA page_size", [], |r| r.get(0))?;
    println!("file: {:.1} MB", (pages * size) as f64 / 1e6);
    Ok(())
}

fn show(db: &str, id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let db = Database::open_read_only(db)?;
    let id = match id.strip_prefix("arimaa:") {
        Some(gid) => db.find_source(ARIMAA_COM, gid)?.ok_or("no such game")?,
        None => id.parse()?,
    };
    let game = db.load(id)?;
    print!("{}", game.to_record().to_record());
    if let Some(e) = &game.replay_error {
        eprintln!("moves stop early: {e}");
    }
    Ok(())
}

fn verify(db: &str) -> Result<(), Box<dyn std::error::Error>> {
    let db = Database::open_read_only(db)?;
    let ids: Vec<i64> = {
        let mut q = db.connection().prepare("SELECT id FROM games ORDER BY id")?;
        q.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?
    };
    let start = Instant::now();
    let (mut failed, mut plies) = (0, 0);
    for &id in &ids {
        match db.load(id) {
            Ok(g) => plies += g.tree.map_or(0, |t| t.main_line().len() - 1),
            Err(e) => {
                failed += 1;
                if failed <= 20 {
                    eprintln!("game {id}: {e}");
                }
            }
        }
    }
    let secs = start.elapsed().as_secs_f64();
    println!(
        "{} games, {plies} plies, {failed} failed, in {secs:.1}s ({:.0} games/s)",
        ids.len(),
        ids.len() as f64 / secs
    );
    Ok(())
}

fn list(
    db: &str,
    player: Option<&String>,
    opponent: Option<&String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let db = Database::open(db)?;
    let filter =
        howdah_db::Filter { player: player.cloned(), opponent: opponent.cloned(), ..Default::default() };
    let start = Instant::now();
    let page = db.search(&filter, 0, 20)?;
    let took = start.elapsed();
    for g in &page.games {
        println!(
            "{:>7} {} {:<16} {:<16} {:>4} {}{}",
            g.id,
            g.date,
            g.gold.as_deref().unwrap_or("?"),
            g.silver.as_deref().unwrap_or("?"),
            g.plies,
            g.winner.unwrap_or('-'),
            g.termination.unwrap_or(' '),
        );
    }
    println!("{} games match; the page took {:.1} ms", page.total, took.as_secs_f64() * 1000.0);
    Ok(())
}

/// The stripped database of the newest state, kept beside the manifest
/// for the next delta.
const STATE_DB: &str = "state.db";

/// Imports archive files into a fresh database at `work`, stripped and
/// vacuumed for a base, and returns its newest date as `YYYY-MM-DD`.
fn build_state(
    work: &std::path::Path,
    rest: &[String],
    start: Instant,
) -> Result<String, Box<dyn std::error::Error>> {
    let (paths, threads) = paths_and_threads(rest)?;
    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(base::work_path(work, suffix));
    }
    let files = archive::archive_files(&paths)?;
    let mut db = Database::open(work)?;
    let report = db.import_archive(&files, threads, |_, _| {})?;
    eprintln!(
        "imported {} games from {} files in {:.0}s",
        report.games,
        report.files,
        start.elapsed().as_secs_f64()
    );
    db.strip_for_base()?;
    db.vacuum()?;
    Ok(db.newest_date()?.unwrap_or_default().replace('.', "-"))
}

fn threads_of(rest: &[String]) -> Result<u32, Box<dyn std::error::Error>> {
    Ok(paths_and_threads(rest)?.1 as u32)
}

fn write_manifest(out_dir: &std::path::Path, m: &base::Manifest) -> Result<(), Box<dyn std::error::Error>> {
    let tmp = out_dir.join("base.json.new");
    std::fs::write(&tmp, serde_json::to_string_pretty(m)? + "\n")?;
    std::fs::rename(tmp, out_dir.join("base.json"))?;
    Ok(())
}

fn describe(path: &std::path::Path, p: &base::Pack, start: Instant) {
    eprintln!(
        "{}: {} games ({} in all) through {}, {:.1} MB ({:.1} MB unpacked), in {:.0}s",
        path.display(),
        p.games,
        p.total,
        p.through.as_deref().unwrap_or("?"),
        p.size as f64 / 1e6,
        p.unpacked_size as f64 / 1e6,
        start.elapsed().as_secs_f64()
    );
}

/// A new full base, with no deltas; its database is kept as the state.
fn base(out_dir: &str, rest: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = PathBuf::from(out_dir);
    std::fs::create_dir_all(&out_dir)?;
    let start = Instant::now();
    let work = out_dir.join("base-work.db");
    let date = build_state(&work, rest, start)?;
    let packed = out_dir.join(format!("howdah-games-{date}.db.zst"));
    let pack = base::pack(&work, &packed, threads_of(rest)?, None, None)?;
    std::fs::rename(&work, out_dir.join(STATE_DB))?;
    write_manifest(&out_dir, &base::Manifest::new(pack.clone()))?;
    describe(&packed, &pack, start);
    Ok(())
}

/// The games added since the state, packed as a delta and added to the
/// manifest; the new import becomes the state.
fn delta(out_dir: &str, rest: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = PathBuf::from(out_dir);
    let state = out_dir.join(STATE_DB);
    let mut manifest: base::Manifest = serde_json::from_slice(&std::fs::read(out_dir.join("base.json"))?)?;
    let old_uuid = Database::open_read_only(&state)?.uuid()?;
    if manifest.newest().uuid != old_uuid {
        return Err(format!("{} isn't the manifest's newest state", state.display()).into());
    }
    let start = Instant::now();
    let work = out_dir.join("delta-work.db");
    let date = build_state(&work, rest, start)?;
    let diff = out_dir.join("delta-diff.db");
    base::make_delta(&state, &work, &diff)?;
    if Database::open_read_only(&diff)?.game_count()? == 0 {
        std::fs::remove_file(&diff)?;
        std::fs::remove_file(&work)?;
        eprintln!("no new games since the last state; nothing written");
        return Ok(());
    }
    let packed = out_dir.join(format!("howdah-games-{date}-delta.db.zst"));
    if packed.exists() {
        std::fs::remove_file(&diff)?;
        std::fs::remove_file(&work)?;
        return Err(format!("{} is there already (a delta through the same date)", packed.display()).into());
    }
    let pack = base::pack(&diff, &packed, threads_of(rest)?, Some(old_uuid), Some(&work))?;
    std::fs::remove_file(&diff)?;
    manifest.deltas.push(pack.clone());
    manifest.check_chain()?;
    write_manifest(&out_dir, &manifest)?;
    std::fs::rename(&work, &state)?;
    describe(&packed, &pack, start);
    Ok(())
}

fn merge(db: &str, other: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut db = Database::open(db)?;
    let start = Instant::now();
    let report = db.merge_from(std::path::Path::new(other), other)?;
    println!("{report:?} in {:.1}s", start.elapsed().as_secs_f64());
    Ok(())
}
