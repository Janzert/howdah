//! Runs engine tests from the command line, on their own or through the
//! daemon.
//!
//! ```text
//! howdah-tourney new SPEC.toml DIR   make a test's directory, snapshotting its engines
//! howdah-tourney run DIR [-j N]      play its games here (resuming), N at once
//! howdah-tourney show DIR|ID         print a test's results (a directory, or the daemon's test)
//!
//! howdah-tourney daemon [--slots N] [--port P] [--memory-budget MB]
//!                                                run the daemon (in the foreground)
//! howdah-tourney submit SPEC.toml [--priority N] add a test to the daemon's queue
//! howdah-tourney list                            the daemon's tests
//! howdah-tourney status                          its slots and running games
//! howdah-tourney watch [ID]                      follow games as they finish (until ID does)
//! howdah-tourney pause|resume|stop ID
//! howdah-tourney archive|delete ID              take a stopped or paused test out
//!                                                (archive zips it into the data directory's archive/)
//! howdah-tourney archived                        the archived tests
//! howdah-tourney restore|delete-archived FILE    put one back in the queue, or delete it
//! howdah-tourney priority ID N
//! howdah-tourney max-pairs ID N|none
//! howdah-tourney undecided-below ID PERCENT|off  end undecided under this chance of deciding in time
//! howdah-tourney concurrency ID N
//! howdah-tourney add-engine ID NAME PROGRAM [ARG...]   add an engine to a pool
//! howdah-tourney add-engine ID ENGINE.toml       ... or one described as in a spec's [[engines]]
//! howdah-tourney retire|unretire ID NAME         stop or restart a pool's engine's new pairs
//! howdah-tourney slots N
//! howdah-tourney shutdown [--now|--cancel]
//! ```
//!
//! `--data-dir DIR` (anywhere) picks the daemon's data directory, otherwise
//! `$HOWDAH_TOURNEY_DIR` or `tourney/` in Howdah's local data directory.
//! The spec format is in `src/spec.rs`; an SPSA tuning run (`[spsa]`)
//! is submitted and run the same way, and `show` and `list` give its
//! iterations and parameters, and a pool (`[pool]`) the same with its
//! ratings, crosstable and outliers. Ctrl-C during `run` or `daemon`
//! starts no more games and waits for the running ones; a second Ctrl-C
//! quits at once (those games are played again next time).
//!
//! The engines' memory budget (all of them together) is half the
//! machine's memory unless `--memory-budget` says otherwise; each test's
//! own limits are in its spec's `[memory]`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use howdah_tourney::client::Client;
use howdah_tourney::{
    Daemon, DaemonEvent, DaemonInfo, Decision, EngineSpec, GameEntry, NO_LIMIT_HORIZON, PoolInfo, Projection,
    RESOURCE_LIMIT, RunEnd, RunEvent, RunOptions, SpsaInfo, StopHandle, Summary, TestDir, TestInfo, TestSpec,
    TestStatus, TourneyError, decide_chance, default_data_dir, pool_info, project, server, spsa_progress,
    summarize,
};

const USAGE: &str = "usage:
  howdah-tourney new SPEC.toml DIR
  howdah-tourney run DIR [-j N]
  howdah-tourney show DIR|ID
  howdah-tourney daemon [--slots N] [--port P] [--memory-budget MB]
  howdah-tourney submit SPEC.toml [--priority N]
  howdah-tourney list | status | watch [ID]
  howdah-tourney pause|resume|stop ID
  howdah-tourney archive|delete ID
  howdah-tourney archived | restore FILE | delete-archived FILE
  howdah-tourney priority ID N
  howdah-tourney max-pairs ID N|none
  howdah-tourney undecided-below ID PERCENT|off
  howdah-tourney concurrency ID N
  howdah-tourney add-engine ID NAME PROGRAM [ARG...] | add-engine ID ENGINE.toml
  howdah-tourney retire|unretire ID NAME
  howdah-tourney slots N
  howdah-tourney shutdown [--now|--cancel]
(any command takes --data-dir DIR for the daemon's data directory)";

#[tokio::main]
async fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut data_dir = None;
    if let Some(i) = args.iter().position(|a| a == "--data-dir") {
        if i + 1 >= args.len() {
            eprintln!("--data-dir needs a directory");
            return ExitCode::FAILURE;
        }
        data_dir = Some(PathBuf::from(args.remove(i + 1)));
        args.remove(i);
    }
    let Some(data_dir) = data_dir.or_else(default_data_dir) else {
        eprintln!("no data directory: give --data-dir");
        return ExitCode::FAILURE;
    };
    match command(&args, &data_dir).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

async fn command(args: &[String], data: &Path) -> Result<(), String> {
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let number =
        |n: &str, what: &str| n.parse::<u32>().map_err(|_| format!("{what} must be a number, not {n:?}"));
    match args.as_slice() {
        ["new", spec, dir] => new(Path::new(spec), Path::new(dir)),
        ["run", dir] => run(Path::new(dir), None).await,
        ["run", dir, "-j", n] | ["run", "-j", n, dir] => match number(n, "-j")? {
            0 => Err("-j must be at least 1".into()),
            n => run(Path::new(dir), Some(n)).await,
        },
        ["show", target] if Path::new(target).join("test.toml").is_file() => show(Path::new(target)),
        ["show", id] => show_remote(data, id).await,
        ["daemon", rest @ ..] => {
            let mut slots = None;
            let mut port = 0;
            let mut budget = None;
            for pair in rest.chunks(2) {
                match pair {
                    ["--slots", n] => slots = Some(number(n, "--slots")?),
                    ["--port", p] => {
                        port = p.parse().map_err(|_| format!("--port must be a port, not {p:?}"))?
                    }
                    ["--memory-budget", mb] => match number(mb, "--memory-budget")? {
                        0 => return Err("--memory-budget must be at least 1 MB".into()),
                        mb => budget = Some(u64::from(mb) << 20),
                    },
                    _ => return Err(USAGE.into()),
                }
            }
            daemon(data, slots, port, budget).await
        }
        ["submit", spec] => submit(data, Path::new(spec), 0).await,
        ["submit", spec, "--priority", p] | ["submit", "--priority", p, spec] => {
            let p = p.parse().map_err(|_| format!("--priority must be a number, not {p:?}"))?;
            submit(data, Path::new(spec), p).await
        }
        ["list"] => list(data).await,
        ["status"] => status(data).await,
        ["watch"] => watch(data, None).await,
        ["watch", id] => watch(data, Some(id)).await,
        [verb @ ("pause" | "resume" | "stop"), id] => {
            let c = client(data)?;
            let info = match *verb {
                "pause" => c.pause(id).await,
                "resume" => c.resume(id).await,
                _ => c.stop(id).await,
            }
            .map_err(|e| e.to_string())?;
            println!("{}", list_line(&info));
            Ok(())
        }
        ["archive", id] => {
            let zip = client(data)?.archive(id).await.map_err(|e| e.to_string())?;
            println!("Archived {id} to {}", zip.display());
            Ok(())
        }
        ["delete", id] => {
            client(data)?.delete(id).await.map_err(|e| e.to_string())?;
            println!("Deleted {id}");
            Ok(())
        }
        ["archived"] => {
            for a in client(data)?.archived().await.map_err(|e| e.to_string())? {
                let size = format!("{:.1} MB", a.size as f64 / 1e6);
                match (&a.test, &a.error) {
                    (Some(t), _) => println!("{}  {size}  {}", a.file, list_line(t)),
                    (None, e) => println!("{}  {size}  unreadable: {}", a.file, e.as_deref().unwrap_or("?")),
                }
            }
            Ok(())
        }
        ["restore", file] => {
            let info = client(data)?.restore(file).await.map_err(|e| e.to_string())?;
            println!("{}", list_line(&info));
            Ok(())
        }
        ["delete-archived", file] => {
            client(data)?.delete_archived(file).await.map_err(|e| e.to_string())?;
            println!("Deleted {file}");
            Ok(())
        }
        ["priority", id, p] => {
            let p = p.parse().map_err(|_| format!("the priority must be a number, not {p:?}"))?;
            let info = client(data)?.set_priority(id, p).await.map_err(|e| e.to_string())?;
            println!("{}", list_line(&info));
            Ok(())
        }
        ["max-pairs", id, n] => {
            let n = if *n == "none" { None } else { Some(number(n, "max pairs")?) };
            let info = client(data)?.set_max_pairs(id, n).await.map_err(|e| e.to_string())?;
            println!("{}", list_line(&info));
            Ok(())
        }
        ["undecided-below", id, p] => {
            let cutoff = match *p {
                "off" => None,
                p => {
                    let pct: f64 = p.trim_end_matches('%').parse().map_err(|_| {
                        format!("give the chance as a percentage (5 for 5%) or off, not {p:?}")
                    })?;
                    Some(pct / 100.0)
                }
            };
            let info = client(data)?.set_undecided_below(id, cutoff).await.map_err(|e| e.to_string())?;
            println!("{}", list_line(&info));
            Ok(())
        }
        ["concurrency", id, n] => {
            let n = number(n, "games at once")?;
            let info = client(data)?.set_concurrency(id, n).await.map_err(|e| e.to_string())?;
            println!("{}", list_line(&info));
            Ok(())
        }
        ["add-engine", id, file] => add_engine(data, id, Path::new(file)).await,
        ["add-engine", id, name, program, args @ ..] => {
            let program = std::path::absolute(program).map_err(|e| e.to_string())?;
            let engine = EngineSpec {
                name: name.to_string(),
                program,
                args: args.iter().map(|a| a.to_string()).collect(),
                working_dir: None,
                options: Default::default(),
                restart_each_game: false,
                copy_dir: false,
                retired: false,
                source: None,
                sha256: None,
            };
            let info = client(data)?.add_engine(id, &engine).await.map_err(|e| e.to_string())?;
            println!("Added {name} to {id}");
            print_pool_or_line(&info);
            Ok(())
        }
        [verb @ ("retire" | "unretire"), id, name] => {
            let retired = *verb == "retire";
            let info =
                client(data)?.set_engine_retired(id, name, retired).await.map_err(|e| e.to_string())?;
            println!("{} {name}", if retired { "Retired" } else { "Brought back" });
            print_pool_or_line(&info);
            Ok(())
        }
        ["slots", n] => {
            let s = client(data)?.set_slots(number(n, "slots")?).await.map_err(|e| e.to_string())?;
            println!("{} slots", s.slots);
            Ok(())
        }
        ["shutdown", "--cancel"] => {
            client(data)?.cancel_shutdown().await.map_err(|e| e.to_string())?;
            println!("Shutdown cancelled");
            Ok(())
        }
        ["shutdown"] | ["shutdown", "--now"] => {
            let now = args.len() == 2;
            client(data)?.shutdown(now).await.map_err(|e| e.to_string())?;
            println!("{}", if now { "Shutting down now" } else { "Shutting down after the running games" });
            Ok(())
        }
        _ => Err(USAGE.into()),
    }
}

fn client(data: &Path) -> Result<Client, String> {
    Client::connect(data).map_err(|e| e.to_string())
}

fn read_spec(spec_path: &Path) -> Result<(TestSpec, PathBuf), String> {
    let text = std::fs::read_to_string(spec_path).map_err(|e| format!("{}: {e}", spec_path.display()))?;
    let spec = TestSpec::parse(&text).map_err(|e| format!("{}: {e}", spec_path.display()))?;
    let base = spec_path.parent().map(Path::to_path_buf).unwrap_or_default();
    let base = if base.as_os_str().is_empty() { PathBuf::from(".") } else { base };
    Ok((spec, base))
}

fn new(spec_path: &Path, dir: &Path) -> Result<(), String> {
    let (spec, base) = read_spec(spec_path)?;
    let test = TestDir::create(spec, &base, dir).map_err(|e| e.to_string())?;
    println!("Made {}", test.path.display());
    print_engines(&test.spec);
    Ok(())
}

fn print_engines(spec: &TestSpec) {
    for e in &spec.engines {
        let source = e.source.as_deref().map(|s| s.display().to_string()).unwrap_or_default();
        println!("  {}: {source} (sha256 {})", e.name, e.sha256.as_deref().unwrap_or("?"));
    }
}

/// Every engine's name, in spec order: a pool's, or a test's two sides.
fn spec_names(spec: &TestSpec) -> Vec<String> {
    match spec.pool {
        Some(_) => spec.engines.iter().map(|e| e.name.clone()).collect(),
        None => spec.side_names().to_vec(),
    }
}

/// [`spec_names`] from a test's info.
fn info_names(t: &TestInfo) -> Vec<String> {
    match &t.pool {
        Some(p) => p.engines.iter().map(|e| e.name.clone()).collect(),
        None => t.engines.to_vec(),
    }
}

/// A test's engines in a heading: `a vs b`, or a pool's.
fn heading(spec: &TestSpec) -> String {
    match spec.pool {
        Some(_) => format!("a pool of {}", spec_names(spec).join(", ")),
        None => spec.side_names().join(" vs "),
    }
}

async fn run(dir: &Path, concurrency: Option<u32>) -> Result<(), String> {
    let test = TestDir::open(dir).map_err(|e| e.to_string())?;
    let names = test.spec.side_names();
    let all = spec_names(&test.spec);
    let done = test.results().map_err(|e| e.to_string())?.len();
    println!("{}: {}", test.spec.name, heading(&test.spec));
    if done > 0 {
        println!("Resuming after {done} games");
    }

    let stop = StopHandle::default();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let sprt = test.spec.sprt;
    let spsa = test.spec.spsa.is_some();
    let mut runner = Box::pin(howdah_tourney::run(
        test,
        RunOptions { concurrency, ..RunOptions::default() },
        stop.clone(),
        tx,
    ));
    let mut interrupts = 0;
    let end = loop {
        tokio::select! {
            end = &mut runner => break end,
            Some(event) = rx.recv() => print_event(&all, &event),
            _ = tokio::signal::ctrl_c() => {
                interrupts += 1;
                if interrupts == 1 {
                    stop.stop();
                    eprintln!("Stopping: finishing the running games (Ctrl-C again to quit now)");
                } else {
                    eprintln!("Quitting; unfinished games will be played on the next run");
                    return Err("interrupted".into());
                }
            }
        }
    };
    while let Ok(event) = rx.try_recv() {
        print_event(&all, &event);
    }
    let end = end.map_err(|e: TourneyError| e.to_string())?;
    let test = TestDir::open(dir).map_err(|e| e.to_string())?;
    let entries = test.results().map_err(|e| e.to_string())?;
    println!();
    match (&test.spec.spsa, &test.spec.pool) {
        (Some(sp), _) => print_spsa(&spsa_progress(&test.path, sp).map_err(|e| e.to_string())?),
        (_, Some(pool)) => print_pool(&pool_info(pool, &test.spec.engines, &entries)),
        _ => print_summary(&names, &summarize(sprt, &entries), sprt.is_some()),
    }
    match end {
        RunEnd::Decided(d) => println!("\nSPRT: {}", decision_text(&names, d)),
        RunEnd::MaxPairs if spsa => println!("\nAll iterations played"),
        RunEnd::MaxPairs => println!("\nAll pairs played"),
        RunEnd::Undecided => println!("\nSPRT unlikely to decide within the pair limit"),
        RunEnd::Stopped => println!("\nStopped"),
        RunEnd::Halted(why) => return Err(format!("\nHalted: {why}")),
    }
    Ok(())
}

fn show(dir: &Path) -> Result<(), String> {
    let test = TestDir::open(dir).map_err(|e| e.to_string())?;
    let names = test.spec.side_names();
    let entries = test.results().map_err(|e| e.to_string())?;
    println!("{}: {}", test.spec.name, heading(&test.spec));
    if let Some(tc) = &test.spec.time_control {
        println!("Time control {tc}");
    }
    if let Some(pool) = &test.spec.pool {
        println!("Games {}", entries.len());
        print_pool(&pool_info(pool, &test.spec.engines, &entries));
        return Ok(());
    }
    if let Some(sp) = &test.spec.spsa {
        println!("Games {}", entries.len());
        print_spsa(&spsa_progress(&test.path, sp).map_err(|e| e.to_string())?);
        return Ok(());
    }
    let summary = summarize(test.spec.sprt, &entries);
    print_summary(&names, &summary, test.spec.sprt.is_some());
    if let Some(d) = summary.decision {
        println!("\nSPRT: {}", decision_text(&names, d));
    } else if let (Some(sp), Some(max)) = (test.spec.sprt, test.spec.max_pairs)
        && let Some(c) = decide_chance(sp, &summary, max)
    {
        println!("Chance the SPRT decides within {max} pairs: {}", percent(c));
    }
    if let Some(sp) = test.spec.sprt
        && let Some(p) = project(sp, &summary, test.spec.max_pairs, sp.undecided_below)
    {
        println!("{}", projection_text(&p, test.spec.max_pairs));
    }
    Ok(())
}

/// A projection in a line: `Likely to end in 1840 games (920 to 3610):
/// pass 22%, fail 36%, undecided 42%`.
fn projection_text(p: &Projection, max_pairs: Option<u32>) -> String {
    let [lo, mid, hi] = p.games_left;
    // Without a pair limit, the longest paths stop at the horizon.
    let open = max_pairs.is_none() && p.pairs_left[2] >= NO_LIMIT_HORIZON;
    let mut line =
        format!("Likely to end in {mid} games ({lo} to {hi}{})", if open { " or more" } else { "" });
    let mut ends = vec![format!("pass {}", percent(p.pass)), format!("fail {}", percent(p.fail))];
    if p.undecided > 0.0 {
        ends.push(format!("undecided {}", percent(p.undecided)));
    }
    if p.done > 0.0 {
        ends.push(match max_pairs {
            Some(_) => format!("every pair played {}", percent(p.done)),
            None => format!("still going {}", percent(p.done)),
        });
    }
    line += &format!(": {}", ends.join(", "));
    line
}

fn print_event(all: &[String], event: &RunEvent) {
    match event {
        RunEvent::GameStarted { .. } | RunEvent::MovePlayed { .. } | RunEvent::EngineOutput { .. } => {}
        RunEvent::GameFinished { entry, summary } => {
            // A pool's summary is engine 1's of every pair: meaningless.
            let summary = (entry.engines.is_none()).then_some(&**summary);
            print_game("", all, entry, summary)
        }
        RunEvent::EngineRestarted { name, reason, .. } => println!("       restarting {name} ({reason})"),
        RunEvent::EngineFailedToStart { name, error, .. } => println!("       {name} didn't start: {error}"),
        RunEvent::EngineRetired { name, reason } => println!("       retired {name}: {reason}"),
    }
}

fn print_game(prefix: &str, all: &[String], entry: &GameEntry, summary: Option<&Summary>) {
    let names = entry.names(all);
    let gold = entry.gold();
    let colour = if entry.winner == gold { "gold" } else { "silver" };
    let mut line = format!(
        "{prefix}{:>4}.{} {} vs {}: {} ({colour}) wins by {} after {} plies",
        entry.pair,
        entry.game,
        names[gold],
        names[1 - gold],
        names[entry.winner],
        if entry.reason == RESOURCE_LIMIT { "going over a memory limit" } else { &entry.reason },
        entry.plies
    );
    if let Some([a, b]) = entry.memory_mb {
        line += &format!(" ({a}/{b} MB)");
    }
    if let Some(summary) = summary.filter(|s| s.pairs > 0) {
        line += &format!(" | {}", short_stats(summary));
    }
    println!("{line}");
    if let Some(detail) = &entry.detail {
        println!("{prefix}       {detail}");
    }
}

fn short_stats(s: &Summary) -> String {
    let mut out = format!("pairs {}, {}", s.pairs, elo_text(s));
    if let (Some(llr), Some((lo, hi))) = (s.llr, s.llr_bounds) {
        out += &format!(", LLR {llr:.2} ({lo:.2}, {hi:.2})");
    }
    out
}

fn elo_text(s: &Summary) -> String {
    match (s.elo, s.elo_interval) {
        (Some(e), Some((lo, hi))) => format!("Elo {} [{}, {}]", signed(e), signed(lo), signed(hi)),
        (Some(e), None) => format!("Elo {}", signed(e)),
        (None, _) => "Elo -".into(),
    }
}

/// A chance as a percentage, without claiming certainty it doesn't have.
fn percent(c: f64) -> String {
    match c * 100.0 {
        p if p > 0.0 && p < 0.1 => "<0.1%".into(),
        p if p < 100.0 && p > 99.9 => ">99.9%".into(),
        p => format!("{p:.1}%"),
    }
}

fn signed(x: f64) -> String {
    if x.is_infinite() {
        if x > 0.0 { "+inf".into() } else { "-inf".into() }
    } else {
        format!("{:+.1}", x + 0.0)
    }
}

fn print_summary(names: &[String; 2], s: &Summary, sprt: bool) {
    println!(
        "Games {} ({} complete pairs): {} {} - {} {}",
        s.games, s.pairs, names[0], s.wins[0], s.wins[1], names[1]
    );
    if s.pairs == 0 {
        return;
    }
    let [l, d, w] = s.pair_counts;
    println!(
        "Pairs: {} won both {w}, split {d}, lost both {l}; score {:.1}%",
        names[0],
        s.score.unwrap_or(0.0) * 100.0
    );
    let level = if s.elo_interval.is_some() { " (95%)" } else { "" };
    println!("{}{level}, LOS {:.1}%", elo_text(s), s.los.unwrap_or(0.0) * 100.0);
    if sprt && let (Some(llr), Some((lo, hi))) = (s.llr, s.llr_bounds) {
        println!("LLR {llr:.2} (bounds {lo:.2}, {hi:.2})");
    }
    let as_gold = |i: usize| s.gold_wins[i];
    let as_silver = |i: usize| s.wins[i] - s.gold_wins[i];
    println!(
        "By colour: {} won {} as gold, {} as silver; {} won {} as gold, {} as silver",
        names[0],
        as_gold(0),
        as_silver(0),
        names[1],
        as_gold(1),
        as_silver(1)
    );
    let reasons: Vec<String> = s.reasons.iter().map(|(r, n)| format!("{r} {n}")).collect();
    println!("Endings: {}", reasons.join(", "));
    if s.repeated > 0 {
        println!(
            "Warning: {} games repeat an earlier game's moves, so the error bars are too narrow",
            s.repeated
        );
    }
}

/// An SPSA run's progress and θ, a parameter a line.
fn print_spsa(s: &SpsaInfo) {
    println!("SPSA: {} of {} iterations done ({} pairs each)", s.done, s.iterations, s.pairs_per_iteration);
    let width = s.params.iter().map(|p| p.name.len()).max().unwrap_or(0);
    for p in &s.params {
        let sent = if p.int { format!(" ~{}", p.value.round()) } else { String::new() };
        println!(
            "  {:width$}  {:>12.4}{sent}  (start {}, range {} to {})",
            p.name, p.value, p.start, p.min, p.max
        );
    }
}

/// A pool's ratings, crosstable and outliers.
fn print_pool(p: &PoolInfo) {
    let Some(anchor) = p.anchor else {
        println!("No games yet");
        return;
    };
    let width = p.engines.iter().map(|e| e.name.len()).max().unwrap_or(0);
    println!("Ratings (Elo relative to {}, with 95% intervals):", p.engines[anchor].name);
    for (rank, &i) in p.ranking.iter().enumerate() {
        let e = &p.engines[i];
        let rating = match (e.elo, e.error) {
            (Some(elo), Some(err)) => format!("{:>7} ±{err:>5.1}", signed(elo)),
            _ => format!("{:>13}", "-"),
        };
        let score = if e.games > 0 {
            format!("{:.1}%", 100.0 * f64::from(e.wins) / f64::from(e.games))
        } else {
            "-".into()
        };
        let retired = if e.retired { "  retired" } else { "" };
        println!(
            "  {:>2}. {:width$}  {rating}  games {:>5}, won {score}{retired}",
            rank + 1,
            e.name,
            e.games
        );
    }
    if p.matchups.is_empty() {
        return;
    }
    // Rows against columns, both in rank order: the row's wins-losses.
    println!("\nCrosstable (row's wins-losses against each column; * out of line):");
    let cell = |a: usize, b: usize| -> String {
        let Some(m) = p.matchups.iter().find(|m| (m.a, m.b) == (a.min(b), a.max(b))) else {
            return "".into();
        };
        let (w, l) = if m.a == a { (m.wins[0], m.wins[1]) } else { (m.wins[1], m.wins[0]) };
        format!("{w}-{l}{}", if m.outlier { "*" } else { "" })
    };
    let cols: Vec<String> = (1..=p.ranking.len()).map(|r| r.to_string()).collect();
    let cw = p
        .ranking
        .iter()
        .flat_map(|&a| p.ranking.iter().map(move |&b| (a, b)))
        .map(|(a, b)| cell(a, b).len())
        .max()
        .unwrap_or(1)
        .max(3);
    println!(
        "  {:>2}  {:width$}  {}",
        "",
        "",
        cols.iter().map(|c| format!("{c:>cw$}")).collect::<Vec<_>>().join(" ")
    );
    for (rank, &a) in p.ranking.iter().enumerate() {
        let row: Vec<String> = p
            .ranking
            .iter()
            .map(|&b| if a == b { format!("{:>cw$}", "·") } else { format!("{:>cw$}", cell(a, b)) })
            .collect();
        println!("  {:>2}. {:width$}  {}", rank + 1, p.engines[a].name, row.join(" "));
    }
    let outliers: Vec<_> = p.matchups.iter().filter(|m| m.outlier).collect();
    if !outliers.is_empty() {
        println!("\nOut of line with the ratings (|z| ≥ {}):", p.outlier_z);
        for m in outliers {
            let n = f64::from(m.wins[0] + m.wins[1]);
            println!(
                "  {} {}-{} {} (the ratings expect {:.1}-{:.1}; z {:+.1})",
                p.engines[m.a].name,
                m.wins[0],
                m.wins[1],
                p.engines[m.b].name,
                m.expected * n,
                (1.0 - m.expected) * n,
                m.z
            );
        }
    }
}

/// A pool's standings, or another test's line.
fn print_pool_or_line(t: &TestInfo) {
    match &t.pool {
        Some(p) => print_pool(p),
        None => println!("{}", list_line(t)),
    }
}

fn decision_text(names: &[String; 2], d: Decision) -> String {
    match d {
        Decision::Pass => format!("passed ({} is stronger)", names[0]),
        Decision::Fail => format!("failed ({} isn't stronger)", names[0]),
    }
}

async fn daemon(data: &Path, slots: Option<u32>, port: u16, budget: Option<u64>) -> Result<(), String> {
    if let Some(info) = DaemonInfo::read(data)
        && Client::new(&info).status().await.is_ok()
    {
        return Err(format!("a daemon is already running for {} (pid {})", data.display(), info.pid));
    }
    let slots = slots.unwrap_or_else(|| {
        let cores = std::thread::available_parallelism().map_or(2, |n| n.get() as u32);
        (cores / 2).max(1)
    });
    let budget = budget.unwrap_or_else(howdah_tourney::default_memory_budget);
    let daemon = Daemon::start_with_budget(data, slots, budget).map_err(|e| e.to_string())?;
    let (listener, info) = server::bind(data, port).await.map_err(|e| e.to_string())?;
    println!("Serving {} on 127.0.0.1:{} with {slots} slots", data.display(), info.port);
    let cap = howdah_aei::process::memory_cap_method()
        .map_or("not capped by the kernel".into(), |m| format!("each capped in {m}"));
    println!("Engine memory: {} MB budget for all of them; {cap}", budget >> 20);
    for t in daemon.list() {
        println!("  {}", list_line(&t));
    }

    let mut events = daemon.subscribe();
    let log = daemon.clone();
    tokio::spawn(async move {
        let mut names: HashMap<String, Vec<String>> = HashMap::new();
        while let Ok(event) = events.recv().await {
            print_daemon_event(&mut names, &event, |id| log.info(id).ok().map(|t| info_names(&t)));
        }
    });
    let signals = daemon.clone();
    tokio::spawn(async move {
        let mut interrupts = 0;
        while tokio::signal::ctrl_c().await.is_ok() {
            interrupts += 1;
            if interrupts == 1 {
                eprintln!("Shutting down: finishing the running games (Ctrl-C again to quit now)");
                signals.shutdown(false);
            } else {
                eprintln!("Quitting; unfinished games will be played next time");
                signals.shutdown(true);
            }
        }
    });
    server::serve(daemon, listener, info).await.map_err(|e: TourneyError| e.to_string())?;
    println!("Shut down");
    Ok(())
}

/// Prints a daemon event as a line of the log `daemon` and `watch` show.
/// `lookup` finds a test's engine names when `names` doesn't have them.
fn print_daemon_event(
    names: &mut HashMap<String, Vec<String>>,
    event: &DaemonEvent,
    lookup: impl Fn(&str) -> Option<Vec<String>>,
) {
    let mut names_of = |id: &str| -> Vec<String> {
        if !names.contains_key(id)
            && let Some(n) = lookup(id)
        {
            names.insert(id.to_string(), n);
        }
        names.get(id).cloned().unwrap_or_default()
    };
    match event {
        DaemonEvent::TestChanged { test } => {
            names.insert(test.id.clone(), info_names(test));
            if test.status != TestStatus::Active && test.running.is_empty() {
                println!("{}", list_line(test));
            }
        }
        DaemonEvent::GameStarted { .. } | DaemonEvent::MovePlayed { .. } => {}
        DaemonEvent::GameFinished { test, entry } => {
            print_game(&format!("{test} "), &names_of(test), entry, None)
        }
        DaemonEvent::EngineRestarted { test, engine, name, reason } => {
            println!("{test}        restarting {} ({reason})", side_name(name, engine, &names_of(test)));
        }
        DaemonEvent::EngineFailedToStart { test, engine, name, error } => {
            println!("{test}        {} didn't start: {error}", side_name(name, engine, &names_of(test)));
        }
        DaemonEvent::EngineRetired { test, name, reason } => {
            println!("{test}        retired {name}: {reason}")
        }
        DaemonEvent::TestRemoved { test, archive } => match archive {
            Some(zip) => println!("{test} archived to {}", zip.display()),
            None => println!("{test} deleted"),
        },
        DaemonEvent::SlotsChanged { slots } => println!("{slots} slots"),
        DaemonEvent::ShuttingDown { now } => {
            println!("{}", if *now { "Shutting down now" } else { "Shutting down after the running games" })
        }
        DaemonEvent::ShutdownCancelled => println!("Shutdown cancelled"),
    }
}

/// An event's engine: its name, or from daemons that didn't send one, its
/// side's.
fn side_name(name: &str, side: &usize, names: &[String]) -> String {
    if !name.is_empty() {
        return name.to_string();
    }
    names.get(*side).cloned().unwrap_or_else(|| format!("engine {}", side + 1))
}

/// Adds an engine described as in a spec's `[[engines]]` to a pool.
async fn add_engine(data: &Path, id: &str, file: &Path) -> Result<(), String> {
    let text = std::fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let mut engine: EngineSpec = toml::from_str(&text).map_err(|e| format!("{}: {e}", file.display()))?;
    let base = file.parent().map(Path::to_path_buf).unwrap_or_default();
    let base = if base.as_os_str().is_empty() { PathBuf::from(".") } else { base };
    absolute_paths(&mut engine, &base)?;
    let info = client(data)?.add_engine(id, &engine).await.map_err(|e| e.to_string())?;
    println!("Added {} to {id}", engine.name);
    print_pool_or_line(&info);
    Ok(())
}

/// An engine's program and working directory as absolute paths, from
/// `base` (the daemon has its own working directory).
fn absolute_paths(e: &mut EngineSpec, base: &Path) -> Result<(), String> {
    e.program = std::path::absolute(base.join(&e.program)).map_err(|err| err.to_string())?;
    if let Some(wd) = &e.working_dir {
        e.working_dir = Some(std::path::absolute(base.join(wd)).map_err(|err| err.to_string())?);
    }
    Ok(())
}

async fn submit(data: &Path, spec_path: &Path, priority: i32) -> Result<(), String> {
    let (mut spec, base) = read_spec(spec_path)?;
    for e in &mut spec.engines {
        absolute_paths(e, &base)?;
    }
    let info = client(data)?.submit(&spec, priority).await.map_err(|e| e.to_string())?;
    println!("Added {} ({})", info.id, info.path.display());
    Ok(())
}

fn list_line(t: &TestInfo) -> String {
    let status = match (&t.status, &t.end) {
        (TestStatus::Active, _) => "active".to_string(),
        (TestStatus::Paused, _) => "paused".to_string(),
        (TestStatus::Finished, Some(RunEnd::Decided(Decision::Pass))) => "passed".to_string(),
        (TestStatus::Finished, Some(RunEnd::Decided(Decision::Fail))) => "failed".to_string(),
        (TestStatus::Finished, Some(RunEnd::MaxPairs)) => "done".to_string(),
        (TestStatus::Finished, Some(RunEnd::Undecided)) => "undecided".to_string(),
        (TestStatus::Finished, Some(RunEnd::Stopped)) => "stopped".to_string(),
        (TestStatus::Finished, Some(RunEnd::Halted(why))) => format!("halted: {why}"),
        (TestStatus::Finished, None) => "finished".to_string(),
    };
    let mut line = match (&t.spsa, &t.pool) {
        (Some(s), _) => format!(
            "{}  SPSA {}  iteration {}/{}, games {}",
            t.id, t.name, s.done, s.iterations, t.summary.games
        ),
        (_, Some(p)) => {
            let active = p.engines.iter().filter(|e| !e.retired).count();
            let mut line =
                format!("{}  pool {}  {active} engines playing, games {}", t.id, t.name, t.summary.games);
            if let Some(&top) = p.ranking.first().filter(|_| p.anchor.is_some()) {
                line += &format!(", leader {}", p.engines[top].name);
            }
            let outliers = p.matchups.iter().filter(|m| m.outlier).count();
            if outliers > 0 {
                line += &format!(", {outliers} out of line");
            }
            line
        }
        _ => format!(
            "{}  {} vs {}  games {}, {}",
            t.id,
            t.engines[0],
            t.engines[1],
            t.summary.games,
            short_stats(&t.summary)
        ),
    };
    if let Some(c) = t.decide_chance.filter(|_| t.status != TestStatus::Finished) {
        line += &format!(", {} to decide in time", percent(c));
    }
    if let Some(p) = &t.projection {
        line += &format!(", about {} games to go", p.games_left[1]);
    }
    if t.priority != 0 {
        line += &format!(", priority {}", t.priority);
    }
    if !t.running.is_empty() {
        line += &format!(", {} running", t.running.len());
    }
    format!("{line}  [{status}]")
}

async fn list(data: &Path) -> Result<(), String> {
    let tests = client(data)?.list().await.map_err(|e| e.to_string())?;
    if tests.is_empty() {
        println!("No tests");
    }
    for t in tests {
        println!("{}", list_line(&t));
    }
    Ok(())
}

async fn status(data: &Path) -> Result<(), String> {
    let s = client(data)?.status().await.map_err(|e| e.to_string())?;
    println!("Daemon pid {} serving {}: {} slots, {} tests", s.pid, s.data_dir.display(), s.slots, s.tests);
    println!(
        "{} engines hold {} MB of a {} MB budget",
        s.engines,
        s.engine_memory >> 20,
        s.memory_budget >> 20
    );
    if s.closing {
        println!("Shutting down");
    }
    for g in &s.running {
        println!("  {} {}.{}: {} (gold) vs {} (silver)", g.test, g.pair, g.game, g.gold, g.silver);
    }
    Ok(())
}

async fn show_remote(data: &Path, id: &str) -> Result<(), String> {
    let t = client(data)?.info(id).await.map_err(|e| e.to_string())?;
    match &t.pool {
        Some(_) => println!("{} ({}): a pool of {}", t.id, t.name, info_names(&t).join(", ")),
        None => println!("{} ({}): {} vs {}", t.id, t.name, t.engines[0], t.engines[1]),
    }
    println!("{}", t.path.display());
    if let Some(tc) = &t.time_control {
        println!("Time control {tc}");
    }
    match (&t.spsa, &t.pool) {
        (Some(s), _) => {
            println!("Games {}", t.summary.games);
            print_spsa(s);
        }
        (_, Some(p)) => {
            println!("Games {}", t.summary.games);
            print_pool(p);
        }
        _ => print_summary(&t.engines, &t.summary, t.sprt.is_some()),
    }
    println!(
        "
{}",
        list_line(&t)
    );
    Ok(())
}

async fn watch(data: &Path, only: Option<&str>) -> Result<(), String> {
    let c = client(data)?;
    let mut stream = c.events().await.map_err(|e| e.to_string())?;
    let mut names: HashMap<String, Vec<String>> =
        c.list().await.map_err(|e| e.to_string())?.iter().map(|t| (t.id.clone(), info_names(t))).collect();
    while let Some(event) = stream.next().await {
        let event = event.map_err(|e| e.to_string())?;
        let test = match &event {
            DaemonEvent::TestChanged { test } => Some(test.id.as_str()),
            DaemonEvent::GameStarted { test, .. }
            | DaemonEvent::MovePlayed { test, .. }
            | DaemonEvent::GameFinished { test, .. }
            | DaemonEvent::EngineRestarted { test, .. }
            | DaemonEvent::EngineFailedToStart { test, .. }
            | DaemonEvent::EngineRetired { test, .. } => Some(test.as_str()),
            _ => None,
        };
        if only.is_some_and(|o| test.is_some_and(|t| t != o)) {
            continue;
        }
        print_daemon_event(&mut names, &event, |_| None);
        let test_over = |t: &TestInfo| {
            only == Some(t.id.as_str()) && t.status != TestStatus::Active && t.running.is_empty()
        };
        match &event {
            DaemonEvent::ShuttingDown { .. } => break,
            DaemonEvent::TestChanged { test } if test_over(test) => break,
            _ => {}
        }
    }
    Ok(())
}
