//! Plays one engine-vs-engine game and prints it.
//!
//! ```text
//! cargo run -p arimaa-aei --example match -- \
//!     --gold "path/to/sharp aei" --silver "path/to/aei-test-engine" \
//!     [--tc 5s/1m] [--gold-dir DIR] [--silver-dir DIR] [--transcript]
//! ```
//!
//! Engine commands are split on whitespace (no shell quoting).

use std::time::Duration;

use arimaa_aei::{Direction, Engine, EngineConfig, Info, MatchConfig, MatchEvent, play_match};
use arimaa_core::Color;

struct Args {
    gold: String,
    silver: String,
    gold_dir: Option<String>,
    silver_dir: Option<String>,
    tc: Option<String>,
    transcript: bool,
}

fn parse_args() -> Args {
    let mut a = Args {
        gold: String::new(),
        silver: String::new(),
        gold_dir: None,
        silver_dir: None,
        tc: None,
        transcript: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        let mut value = || it.next().unwrap_or_else(|| panic!("{flag} needs a value"));
        match flag.as_str() {
            "--gold" => a.gold = value(),
            "--silver" => a.silver = value(),
            "--gold-dir" => a.gold_dir = Some(value()),
            "--silver-dir" => a.silver_dir = Some(value()),
            "--tc" => a.tc = Some(value()),
            "--transcript" => a.transcript = true,
            other => panic!("unknown argument {other}"),
        }
    }
    assert!(!a.gold.is_empty() && !a.silver.is_empty(), "--gold and --silver are required");
    a
}

fn engine_config(command: &str, dir: &Option<String>) -> EngineConfig {
    let mut parts = command.split_whitespace();
    let mut config = EngineConfig::new(parts.next().expect("empty engine command")).args(parts);
    if let Some(d) = dir {
        config = config.working_dir(d);
    }
    config
}

async fn start(command: &str, dir: &Option<String>, label: &'static str, transcript: bool) -> Engine {
    let config = engine_config(command, dir);
    let log = transcript.then(|| {
        Box::new(move |d: Direction, line: &str| {
            let arrow = if d == Direction::ToEngine { ">" } else { "<" };
            eprintln!("  [{label} {arrow}] {line}");
        }) as Box<dyn FnMut(Direction, &str) + Send>
    });
    Engine::start_with_transcript(&config, log).await.unwrap_or_else(|e| panic!("{label}: {e}"))
}

fn fmt_dur(d: Duration) -> String {
    format!("{:.1}s", d.as_secs_f64())
}

#[tokio::main]
async fn main() {
    let args = parse_args();
    let mut gold = start(&args.gold, &args.gold_dir, "gold", args.transcript).await;
    let mut silver = start(&args.silver, &args.silver_dir, "silver", args.transcript).await;
    let config = MatchConfig {
        time_control: args.tc.as_deref().map(|t| t.parse().expect("invalid --tc")),
        ..MatchConfig::default()
    };
    if let Some(tc) = config.time_control {
        println!("Time control {tc}");
    }

    // Last depth and score reported during the current turn.
    let mut depth = None;
    let mut score = None;
    let outcome = play_match(&mut gold, &mut silver, &config, |event| match event {
        MatchEvent::Started { gold, silver } => {
            let name = |id: &arimaa_aei::EngineId| id.name.clone().unwrap_or_else(|| "?".into());
            println!("{} (gold) vs {} (silver)", name(&gold), name(&silver));
        }
        MatchEvent::TurnStarted { .. } => {
            depth = None;
            score = None;
        }
        MatchEvent::Info { info: Info::Depth { depth: d, .. }, .. } => depth = Some(d),
        MatchEvent::Info { info: Info::Score(s), .. } => score = Some(s),
        MatchEvent::Unexpected { side, line } => eprintln!("  {side:?} sent unexpected: {line}"),
        MatchEvent::MovePlayed { ply, side, notation, used, reserve } => {
            let label = arimaa_core::notation::move_label(ply);
            let mut extra = vec![fmt_dur(used)];
            if let Some(r) = reserve {
                extra.push(format!("reserve {}", fmt_dur(r)));
            }
            if let Some(d) = depth {
                extra.push(format!("depth {d}"));
            }
            if let Some(s) = score {
                // Scores are from the mover's view; show them from gold's.
                let gold_view = if side == Color::Gold { s } else { -s };
                extra.push(format!("eval {gold_view:+}"));
            }
            println!("{label} {notation}  ({})", extra.join(", "));
        }
        _ => {}
    })
    .await
    .expect("match failed to start");

    println!(
        "\nResult: {:?} wins by {:?} ('{}{}')",
        outcome.result.winner,
        outcome.result.reason,
        outcome.result.winner.letter(),
        outcome.result.reason.letter()
    );
    if let Some(detail) = outcome.detail {
        println!("  {detail}");
    }
    println!("\n{}", outcome.game.to_record());
    gold.quit(Duration::from_secs(5)).await.ok();
    silver.quit(Duration::from_secs(5)).await.ok();
}
