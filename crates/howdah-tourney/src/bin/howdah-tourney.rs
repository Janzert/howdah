//! Runs engine tests from the command line.
//!
//! ```text
//! howdah-tourney new SPEC.toml DIR   make a test's directory, snapshotting its engines
//! howdah-tourney run DIR [-j N]      play its games (resuming), N at once
//! howdah-tourney show DIR            print its results so far
//! ```
//!
//! The spec format is in `src/spec.rs`. Ctrl-C during `run` starts no more
//! games and waits for the running ones; a second Ctrl-C quits at once
//! (those games are played again on the next run).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use howdah_tourney::{
    Decision, RunEnd, RunEvent, RunOptions, StopHandle, Summary, TestDir, TestSpec, TourneyError, summarize,
};

const USAGE: &str = "usage:
  howdah-tourney new SPEC.toml DIR
  howdah-tourney run DIR [-j N]
  howdah-tourney show DIR";

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["new", spec, dir] => new(Path::new(spec), Path::new(dir)),
        ["run", dir] => run(Path::new(dir), None).await,
        ["run", dir, "-j", n] | ["run", "-j", n, dir] => match n.parse() {
            Ok(n) if n > 0 => run(Path::new(dir), Some(n)).await,
            _ => Err(format!("-j needs a number of games, not {n:?}")),
        },
        ["show", dir] => show(Path::new(dir)),
        _ => Err(USAGE.into()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn new(spec_path: &Path, dir: &Path) -> Result<(), String> {
    let text = std::fs::read_to_string(spec_path).map_err(|e| format!("{}: {e}", spec_path.display()))?;
    let spec = TestSpec::parse(&text).map_err(|e| format!("{}: {e}", spec_path.display()))?;
    let base = spec_path.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    let base = if base.as_os_str().is_empty() { PathBuf::from(".") } else { base };
    let test = TestDir::create(spec, &base, dir).map_err(|e| e.to_string())?;
    println!("Made {}", test.path.display());
    for e in &test.spec.engines {
        let source = e.source.as_deref().map(|s| s.display().to_string()).unwrap_or_default();
        println!("  {}: {source} (sha256 {})", e.name, e.sha256.as_deref().unwrap_or("?"));
    }
    Ok(())
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
        RunEvent::GameStarted { .. } => {}
        RunEvent::GameFinished { entry, summary } => {
            let gold = entry.gold();
            let colour = if entry.winner == gold { "gold" } else { "silver" };
            let mut line = format!(
                "{:>4}.{} {} vs {}: {} ({colour}) wins by {} after {} plies",
                entry.pair,
                entry.game,
                names[gold],
                names[1 - gold],
                names[entry.winner],
                entry.reason,
                entry.plies
            );
            if summary.pairs > 0 {
                line += &format!(" | {}", short_stats(summary));
            }
            println!("{line}");
            if let Some(detail) = &entry.detail {
                println!("       {detail}");
            }
        }
        RunEvent::EngineRestarted { engine, reason } => {
            println!("       restarting {} ({reason})", names[*engine]);
        }
        RunEvent::EngineFailedToStart { engine, error } => {
            println!("       {} didn't start: {error}", names[*engine]);
        }
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
