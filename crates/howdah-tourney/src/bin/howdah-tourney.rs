//! Runs engine tests from the command line, on their own or through the
//! daemon.
//!
//! ```text
//! howdah-tourney new SPEC.toml DIR   make a test's directory, snapshotting its engines
//! howdah-tourney run DIR [-j N]      play its games here (resuming), N at once
//! howdah-tourney show DIR|ID         print a test's results (a directory, or the daemon's test)
//!
//! howdah-tourney daemon [--slots N] [--port P]   run the daemon (in the foreground)
//! howdah-tourney submit SPEC.toml [--priority N] add a test to the daemon's queue
//! howdah-tourney list                            the daemon's tests
//! howdah-tourney status                          its slots and running games
//! howdah-tourney watch [ID]                      follow games as they finish (until ID does)
//! howdah-tourney pause|resume|stop ID
//! howdah-tourney priority ID N
//! howdah-tourney max-pairs ID N|none
//! howdah-tourney slots N
//! howdah-tourney shutdown [--now]
//! ```
//!
//! `--data-dir DIR` (anywhere) picks the daemon's data directory, otherwise
//! `$HOWDAH_TOURNEY_DIR` or `tourney/` in Howdah's local data directory.
//! The spec format is in `src/spec.rs`. Ctrl-C during `run` or `daemon`
//! starts no more games and waits for the running ones; a second Ctrl-C
//! quits at once (those games are played again next time).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use howdah_tourney::client::Client;
use howdah_tourney::{
    Daemon, DaemonEvent, DaemonInfo, Decision, GameEntry, RunEnd, RunEvent, RunOptions, StopHandle, Summary,
    TestDir, TestInfo, TestSpec, TestStatus, TourneyError, default_data_dir, server, summarize,
};

const USAGE: &str = "usage:
  howdah-tourney new SPEC.toml DIR
  howdah-tourney run DIR [-j N]
  howdah-tourney show DIR|ID
  howdah-tourney daemon [--slots N] [--port P]
  howdah-tourney submit SPEC.toml [--priority N]
  howdah-tourney list | status | watch [ID]
  howdah-tourney pause|resume|stop ID
  howdah-tourney priority ID N
  howdah-tourney max-pairs ID N|none
  howdah-tourney slots N
  howdah-tourney shutdown [--now]
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
            for pair in rest.chunks(2) {
                match pair {
                    ["--slots", n] => slots = Some(number(n, "--slots")?),
                    ["--port", p] => {
                        port = p.parse().map_err(|_| format!("--port must be a port, not {p:?}"))?
                    }
                    _ => return Err(USAGE.into()),
                }
            }
            daemon(data, slots, port).await
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
        ["slots", n] => {
            let s = client(data)?.set_slots(number(n, "slots")?).await.map_err(|e| e.to_string())?;
            println!("{} slots", s.slots);
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

async fn run(dir: &Path, concurrency: Option<u32>) -> Result<(), String> {
    let test = TestDir::open(dir).map_err(|e| e.to_string())?;
    let names = [test.spec.engines[0].name.clone(), test.spec.engines[1].name.clone()];
    let done = test.results().map_err(|e| e.to_string())?.len();
    println!("{}: {} vs {}", test.spec.name, names[0], names[1]);
    if done > 0 {
        println!("Resuming after {done} games");
    }

    let stop = StopHandle::default();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let sprt = test.spec.sprt;
    let mut runner = Box::pin(howdah_tourney::run(test, RunOptions { concurrency }, stop.clone(), tx));
    let mut interrupts = 0;
    let end = loop {
        tokio::select! {
            end = &mut runner => break end,
            Some(event) = rx.recv() => print_event(&names, &event),
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
        print_event(&names, &event);
    }
    let end = end.map_err(|e: TourneyError| e.to_string())?;
    let test = TestDir::open(dir).map_err(|e| e.to_string())?;
    let entries = test.results().map_err(|e| e.to_string())?;
    println!();
    print_summary(&names, &summarize(sprt, &entries), sprt.is_some());
    match end {
        RunEnd::Decided(d) => println!("\nSPRT: {}", decision_text(&names, d)),
        RunEnd::MaxPairs => println!("\nAll pairs played"),
        RunEnd::Stopped => println!("\nStopped"),
        RunEnd::Halted(why) => return Err(format!("\nHalted: {why}")),
    }
    Ok(())
}

fn show(dir: &Path) -> Result<(), String> {
    let test = TestDir::open(dir).map_err(|e| e.to_string())?;
    let names = [test.spec.engines[0].name.clone(), test.spec.engines[1].name.clone()];
    let entries = test.results().map_err(|e| e.to_string())?;
    println!("{}: {} vs {}", test.spec.name, names[0], names[1]);
    if let Some(tc) = &test.spec.time_control {
        println!("Time control {tc}");
    }
    let summary = summarize(test.spec.sprt, &entries);
    print_summary(&names, &summary, test.spec.sprt.is_some());
    if let Some(d) = summary.decision {
        println!("\nSPRT: {}", decision_text(&names, d));
    }
    Ok(())
}

fn print_event(names: &[String; 2], event: &RunEvent) {
    match event {
        RunEvent::GameStarted { .. } | RunEvent::MovePlayed { .. } => {}
        RunEvent::GameFinished { entry, summary } => print_game("", names, entry, Some(summary)),
        RunEvent::EngineRestarted { engine, reason } => {
            println!("       restarting {} ({reason})", names[*engine]);
        }
        RunEvent::EngineFailedToStart { engine, error } => {
            println!("       {} didn't start: {error}", names[*engine]);
        }
    }
}

fn print_game(prefix: &str, names: &[String; 2], entry: &GameEntry, summary: Option<&Summary>) {
    let gold = entry.gold();
    let colour = if entry.winner == gold { "gold" } else { "silver" };
    let mut line = format!(
        "{prefix}{:>4}.{} {} vs {}: {} ({colour}) wins by {} after {} plies",
        entry.pair,
        entry.game,
        names[gold],
        names[1 - gold],
        names[entry.winner],
        entry.reason,
        entry.plies
    );
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

fn decision_text(names: &[String; 2], d: Decision) -> String {
    match d {
        Decision::Pass => format!("passed ({} is stronger)", names[0]),
        Decision::Fail => format!("failed ({} isn't stronger)", names[0]),
    }
}

async fn daemon(data: &Path, slots: Option<u32>, port: u16) -> Result<(), String> {
    if let Some(info) = DaemonInfo::read(data)
        && Client::new(&info).status().await.is_ok()
    {
        return Err(format!("a daemon is already running for {} (pid {})", data.display(), info.pid));
    }
    let slots = slots.unwrap_or_else(|| {
        let cores = std::thread::available_parallelism().map_or(2, |n| n.get() as u32);
        (cores / 2).max(1)
    });
    let daemon = Daemon::start(data, slots).map_err(|e| e.to_string())?;
    let (listener, info) = server::bind(data, port).await.map_err(|e| e.to_string())?;
    println!("Serving {} on 127.0.0.1:{} with {slots} slots", data.display(), info.port);
    for t in daemon.list() {
        println!("  {}", list_line(&t));
    }

    let mut events = daemon.subscribe();
    let log = daemon.clone();
    tokio::spawn(async move {
        let mut names: HashMap<String, [String; 2]> = HashMap::new();
        while let Ok(event) = events.recv().await {
            print_daemon_event(&mut names, &event, |id| log.info(id).ok().map(|t| t.engines));
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
    names: &mut HashMap<String, [String; 2]>,
    event: &DaemonEvent,
    lookup: impl Fn(&str) -> Option<[String; 2]>,
) {
    let mut names_of = |id: &str| -> [String; 2] {
        if !names.contains_key(id)
            && let Some(n) = lookup(id)
        {
            names.insert(id.to_string(), n);
        }
        names.get(id).cloned().unwrap_or_else(|| ["engine 1".into(), "engine 2".into()])
    };
    match event {
        DaemonEvent::TestChanged { test } => {
            names.insert(test.id.clone(), test.engines.clone());
            if test.status != TestStatus::Active && test.running.is_empty() {
                println!("{}", list_line(test));
            }
        }
        DaemonEvent::GameStarted { .. } | DaemonEvent::MovePlayed { .. } => {}
        DaemonEvent::GameFinished { test, entry } => {
            print_game(&format!("{test} "), &names_of(test), entry, None)
        }
        DaemonEvent::EngineRestarted { test, engine, reason } => {
            println!("{test}        restarting {} ({reason})", names_of(test)[*engine]);
        }
        DaemonEvent::EngineFailedToStart { test, engine, error } => {
            println!("{test}        {} didn't start: {error}", names_of(test)[*engine]);
        }
        DaemonEvent::SlotsChanged { slots } => println!("{slots} slots"),
        DaemonEvent::ShuttingDown { now } => {
            println!("{}", if *now { "Shutting down now" } else { "Shutting down after the running games" })
        }
    }
}

async fn submit(data: &Path, spec_path: &Path, priority: i32) -> Result<(), String> {
    let (mut spec, base) = read_spec(spec_path)?;
    // The daemon has its own working directory.
    for e in &mut spec.engines {
        e.program = std::path::absolute(base.join(&e.program)).map_err(|err| err.to_string())?;
        if let Some(wd) = &e.working_dir {
            e.working_dir = Some(std::path::absolute(base.join(wd)).map_err(|err| err.to_string())?);
        }
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
        (TestStatus::Finished, Some(RunEnd::Stopped)) => "stopped".to_string(),
        (TestStatus::Finished, Some(RunEnd::Halted(why))) => format!("halted: {why}"),
        (TestStatus::Finished, None) => "finished".to_string(),
    };
    let mut line = format!(
        "{}  {} vs {}  games {}, {}",
        t.id,
        t.engines[0],
        t.engines[1],
        t.summary.games,
        short_stats(&t.summary)
    );
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
    println!("{} ({}): {} vs {}", t.id, t.name, t.engines[0], t.engines[1]);
    println!("{}", t.path.display());
    if let Some(tc) = &t.time_control {
        println!("Time control {tc}");
    }
    print_summary(&t.engines, &t.summary, t.sprt.is_some());
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
    let mut names: HashMap<String, [String; 2]> =
        c.list().await.map_err(|e| e.to_string())?.into_iter().map(|t| (t.id, t.engines)).collect();
    while let Some(event) = stream.next().await {
        let event = event.map_err(|e| e.to_string())?;
        let test = match &event {
            DaemonEvent::TestChanged { test } => Some(test.id.as_str()),
            DaemonEvent::GameStarted { test, .. }
            | DaemonEvent::MovePlayed { test, .. }
            | DaemonEvent::GameFinished { test, .. }
            | DaemonEvent::EngineRestarted { test, .. }
            | DaemonEvent::EngineFailedToStart { test, .. } => Some(test.as_str()),
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
