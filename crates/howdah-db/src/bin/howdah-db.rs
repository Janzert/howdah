//! The game database's command line.
//!
//! ```text
//! howdah-db import DB PATH... [-j N]   arimaa.com archive files (or directories of them)
//! howdah-db stats DB                   counts and sizes
//! howdah-db show DB ID                 a game's record (`ID`, or `arimaa:ID` for a permanent id)
//! howdah-db verify DB                  loads every game back, reporting any that fail
//! ```

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use howdah_db::{ARIMAA_COM, Database, archive};

fn usage() -> ExitCode {
    eprintln!("usage: howdah-db import DB PATH... [-j N] | stats DB | show DB ID|arimaa:ID | verify DB");
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("import") if args.len() >= 3 => import(&args[1], &args[2..]),
        Some("stats") if args.len() == 2 => stats(&args[1]),
        Some("show") if args.len() == 3 => show(&args[1], &args[2]),
        Some("verify") if args.len() == 2 => verify(&args[1]),
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

fn import(db: &str, rest: &[String]) -> Result<(), Box<dyn std::error::Error>> {
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
