//! The game database's command line.
//!
//! ```text
//! howdah-db import DB PATH... [-j N]   arimaa.com archive files (or directories of them)
//! howdah-db stats DB                   counts and sizes
//! howdah-db show DB ID                 a game's record (`ID`, or `arimaa:ID` for a permanent id)
//! howdah-db list DB [PLAYER [OPPONENT]]   the newest 20 games, timed
//! howdah-db setups DB [gold|silver|KEY] [PLAYER]
//!                                       the 20 most played setups (gold's, silver's, or the
//!                                       replies to gold setup KEY), timed
//! howdah-db publish OUTDIR PATH... [--new-base] [--cap PCT] [-j N]
//!                                       the base database and its deltas from archive files,
//!                                       with OUTDIR/base.json and OUTDIR/seen.txt (keep both)
//! howdah-db merge DB OTHER              merges another database file into DB
//! howdah-db verify DB                  loads every game back, reporting any that fail
//! ```

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use howdah_db::{ARIMAA_COM, Database, archive, base};

fn usage() -> ExitCode {
    eprintln!(
        "usage: howdah-db import DB PATH... [-j N] | stats DB | show DB ID|arimaa:ID | verify DB | list DB [PLAYER [OPPONENT]] | setups DB [gold|silver|KEY] [PLAYER] | publish OUTDIR PATH... [--new-base] [--cap PCT] | merge DB OTHER"
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
        Some("publish") if args.len() >= 3 => publish(&args[1], &args[2..]),
        Some("merge") if args.len() == 3 => merge(&args[1], &args[2]),
        Some("list") if (2..=4).contains(&args.len()) => list(&args[1], args.get(2), args.get(3)),
        Some("setups") if (2..=4).contains(&args.len()) => setups(&args[1], args.get(2), args.get(3)),
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

fn setups(
    db: &str,
    level: Option<&String>,
    player: Option<&String>,
) -> Result<(), Box<dyn std::error::Error>> {
    use howdah_db::SetupLevel;
    let db = Database::open(db)?;
    let level = match level.map(String::as_str) {
        None | Some("gold") => SetupLevel::Gold,
        Some("silver") => SetupLevel::Silver,
        Some(key) => SetupLevel::Replies(key.to_string()),
    };
    let filter = howdah_db::Filter { player: player.cloned(), ..Default::default() };
    let start = Instant::now();
    let stats = db.setup_stats(&filter, &level, 20)?;
    let took = start.elapsed();
    for r in &stats.rows {
        let pct = |n: f64| 100.0 * n / r.rated_games.max(1) as f64;
        println!(
            "{} {:>7} games, gold {:>5.1}% (rated {:>5.1}%, expected {:>5.1}%), rating {:>4.0}",
            r.setup,
            r.games,
            100.0 * r.gold_wins as f64 / r.games as f64,
            pct(r.rated_gold_wins as f64),
            pct(r.expected_gold_wins),
            r.rating.unwrap_or(0.0),
        );
    }
    println!("{} games, {} setups; took {:.1} ms", stats.games, stats.setups, took.as_secs_f64() * 1000.0);
    Ok(())
}

/// Imports archive files into a fresh database, strips and vacuums it,
/// and publishes it into `OUTDIR` (`base::publish`): the base when asked
/// or when there's none, the deltas, and `base.json`.
fn publish(out_dir: &str, rest: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let mut new_base = false;
    let mut cap_percent = 10;
    let mut others = Vec::new();
    let mut it = rest.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--new-base" => new_base = true,
            "--cap" => cap_percent = it.next().ok_or("--cap needs a percentage")?.parse()?,
            _ => others.push(a.clone()),
        }
    }
    let (paths, threads) = paths_and_threads(&others)?;
    let out_dir = PathBuf::from(out_dir);
    std::fs::create_dir_all(&out_dir)?;
    let start = Instant::now();
    let work = out_dir.join("import-work.db");
    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(base::work_path(&work, suffix));
    }
    let files = archive::archive_files(&paths)?;
    {
        let mut db = Database::open(&work)?;
        let report = db.import_archive(&files, threads, |_, _| {})?;
        eprintln!(
            "imported {} games from {} files in {:.0}s",
            report.games,
            report.files,
            start.elapsed().as_secs_f64()
        );
        db.strip_for_base()?;
        db.vacuum()?;
    }
    let opts = base::PublishOptions {
        at: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs() as i64,
        new_base,
        cap_percent,
        threads: threads as u32,
    };
    let published = base::publish(&out_dir, &work, &opts);
    std::fs::remove_file(&work)?;
    let Some(p) = published? else {
        eprintln!("no new games since the last publish; nothing written");
        return Ok(());
    };
    let m = &p.manifest;
    eprintln!(
        "{} new games; {} in all through {}, in {:.0}s",
        p.new_games,
        m.games,
        m.through.as_deref().unwrap_or("?"),
        start.elapsed().as_secs_f64()
    );
    for pack in std::iter::once(&m.base).chain(&m.deltas) {
        eprintln!("  {}: {} games, {:.2} MB", pack.file, pack.games, pack.size as f64 / 1e6);
    }
    if !p.dropped.is_empty() {
        eprintln!("no longer named (take them down once base.json is up):");
        for f in &p.dropped {
            eprintln!("  {f}");
        }
    }
    Ok(())
}

fn merge(db: &str, other: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut db = Database::open(db)?;
    let start = Instant::now();
    let report = db.merge_from(std::path::Path::new(other), other)?;
    println!("{report:?} in {:.1}s", start.elapsed().as_secs_f64());
    Ok(())
}
