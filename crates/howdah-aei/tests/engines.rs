//! Integration tests against the bundled `aei-test-engine`.

use std::time::Duration;

use howdah_aei::{AeiError, Engine, EngineConfig, EngineMessage, MatchConfig, MatchEvent, play_match};
use howdah_arimaa::{Color, Game, WinReason};

const TEST_ENGINE: &str = env!("CARGO_BIN_EXE_aei-test-engine");

fn config(args: &[&str]) -> EngineConfig {
    EngineConfig::new(TEST_ENGINE).args(args.iter().copied())
}

async fn start(args: &[&str]) -> Engine {
    Engine::start(&config(args)).await.expect("engine starts")
}

#[tokio::test]
async fn handshake_reports_identity() {
    let engine = start(&["--mode", "random"]).await;
    assert_eq!(engine.id().protocol_version, 1);
    assert_eq!(engine.name(), "aei-test-engine");
    assert_eq!(engine.id().version.as_deref(), Some("random"));
    engine.quit(Duration::from_secs(5)).await.unwrap();
}

#[tokio::test]
async fn missing_program_is_a_spawn_error() {
    let err = Engine::start(&EngineConfig::new("/nonexistent/engine")).await.err().unwrap();
    assert!(matches!(err, AeiError::Spawn { .. }));
}

#[tokio::test]
async fn missing_working_dir_is_reported_as_such() {
    let err = Engine::start(&config(&[]).working_dir("/nonexistent/dir")).await.err().unwrap();
    assert!(matches!(err, AeiError::WorkingDir(_)), "{err}");
}

#[tokio::test]
async fn setposition_and_go() {
    let mut engine = start(&["--seed", "7"]).await;
    engine.new_game().await.unwrap();
    let pos = howdah_arimaa::Position::from_short_string(
        Color::Gold,
        "[rrrrrrrrhdcemcdh                                HDCMECDHRRRRRRRR]",
    )
    .unwrap();
    engine.set_position(&pos).await.unwrap();
    engine.is_ready(Duration::from_secs(5)).await.unwrap();
    engine.go().await.unwrap();
    let mv = loop {
        if let EngineMessage::BestMove(m) = engine.recv().await.unwrap() {
            break m;
        }
    };
    // The move must be legal from that position.
    let mut tb = howdah_arimaa::TurnBuilder::new(&pos);
    let (body, _) = howdah_arimaa::notation::parse_move_body(&mv).unwrap();
    let howdah_arimaa::notation::MoveBody::Steps(steps) = body else { panic!("expected steps, got {mv}") };
    for s in steps {
        tb.try_step(s.step).unwrap();
    }
    tb.finish().unwrap();
}

async fn run(
    gold: &[&str],
    silver: &[&str],
    tc: Option<&str>,
) -> (howdah_aei::MatchOutcome, Vec<MatchEvent>) {
    let mut g = start(gold).await;
    let mut s = start(silver).await;
    let config = MatchConfig {
        time_control: tc.map(|t| t.parse().unwrap()),
        stop_margin: Duration::from_millis(300),
        ..MatchConfig::default()
    };
    let mut events = Vec::new();
    let outcome = play_match(&mut g, &mut s, &config, |e| events.push(e)).await.unwrap();
    (outcome, events)
}

#[tokio::test(flavor = "multi_thread")]
async fn random_engines_finish_a_legal_game() {
    let (outcome, events) = run(&["--seed", "11"], &["--seed", "12"], None).await;
    assert!(outcome.game.ply_count() > 2);
    // The record replays through the core parser.
    let replay = Game::parse(&outcome.game.to_record()).unwrap();
    assert_eq!(replay.current_position(), outcome.game.current_position());
    // Positional endings are found by the core; this game ended on the board.
    assert!(matches!(
        outcome.result.reason,
        WinReason::Goal | WinReason::Elimination | WinReason::Immobilization | WinReason::Resignation
    ));
    assert!(matches!(events[0], MatchEvent::Started { .. }));
    let moves = events.iter().filter(|e| matches!(e, MatchEvent::MovePlayed { .. })).count();
    assert_eq!(moves, outcome.game.ply_count());
    assert!(events.iter().any(|e| matches!(e, MatchEvent::Info { .. })));
    assert!(events.iter().any(|e| matches!(e, MatchEvent::Log { .. })));
}

#[tokio::test(flavor = "multi_thread")]
async fn illegal_move_loses() {
    // Silver's first turn (after both setups) is illegal.
    let (outcome, _) = run(&[], &["--mode", "illegal", "--after", "1"], None).await;
    assert_eq!(outcome.result.winner, Color::Gold);
    assert_eq!(outcome.result.reason, WinReason::IllegalMove);
    assert!(outcome.detail.unwrap().contains("Eh8n"));
    assert_eq!(outcome.game.ply_count(), 3);
}

#[tokio::test(flavor = "multi_thread")]
async fn crash_forfeits() {
    let (outcome, _) = run(&["--mode", "crash", "--after", "2"], &[], None).await;
    assert_eq!(outcome.result.winner, Color::Silver);
    assert_eq!(outcome.result.reason, WinReason::Forfeit);
    assert_eq!(outcome.game.ply_count(), 4, "gold's setup, silver's setup, one turn each");
}

#[tokio::test(flavor = "multi_thread")]
async fn resignation() {
    let (outcome, _) = run(&[], &["--mode", "resign", "--after", "1"], None).await;
    assert_eq!(outcome.result.winner, Color::Gold);
    assert_eq!(outcome.result.reason, WinReason::Resignation);
}

#[tokio::test(flavor = "multi_thread")]
async fn garbage_is_reported_not_fatal() {
    let (outcome, events) =
        run(&["--mode", "garbage", "--after", "1", "--seed", "3"], &["--seed", "4"], None).await;
    assert!(outcome.game.ply_count() > 3);
    assert!(events.iter().any(
        |e| matches!(e, MatchEvent::Unexpected { side: Color::Gold, line } if line.contains("not an AEI message"))
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn hanging_engine_times_out() {
    // 1 s per move, no reserve: silver's first real turn never answers.
    let (outcome, events) = run(&[], &["--mode", "hang", "--after", "1"], Some("1s/0")).await;
    assert_eq!(outcome.result, howdah_arimaa::GameResult { winner: Color::Gold, reason: WinReason::Timeout });
    let starts: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            MatchEvent::TurnStarted { allowance, .. } => *allowance,
            _ => None,
        })
        .collect();
    // The two setups get a minute each; then 1 s per turn.
    assert_eq!(starts[..2], [Duration::from_secs(60); 2]);
    assert!(starts[2..].iter().all(|a| *a == Duration::from_secs(1)));
}

#[tokio::test(flavor = "multi_thread")]
async fn slow_engine_is_stopped_before_the_deadline() {
    // Silver would take 10 s, but gets `stop` 300 ms before its 2 s deadline.
    let (outcome, events) = run(
        &["--seed", "5"],
        &["--mode", "slow", "--after", "1", "--delay-ms", "10000", "--seed", "6"],
        Some("2s/0/100/0/0/0"),
    )
    .await;
    let slow_turn = events
        .iter()
        .find_map(|e| match e {
            MatchEvent::MovePlayed { ply: 3, used, .. } => Some(*used),
            _ => None,
        })
        .expect("silver's first turn was played");
    assert!(slow_turn >= Duration::from_millis(1500) && slow_turn < Duration::from_secs(2), "{slow_turn:?}");
    assert_ne!(outcome.result.reason, WinReason::Timeout, "stop got a move in: {:?}", outcome.detail);
}

#[tokio::test(flavor = "multi_thread")]
async fn turn_limit_ends_by_score() {
    let (outcome, _) = run(&["--seed", "21"], &["--seed", "22"], Some("30s/0/100/0/3t")).await;
    // Setups are turn 1, so 3 turns = 6 plies, unless it ended sooner.
    if outcome.result.reason == WinReason::Score {
        assert_eq!(outcome.game.ply_count(), 6);
    }
    assert!(outcome.game.ply_count() <= 6);
}

/// An engine dies with the thread that spawned it, even when no destructor
/// runs and its stdin stays open (as if it ignored end of input).
#[cfg(target_os = "linux")]
#[test]
fn engine_dies_with_its_spawning_thread() {
    let pid = std::thread::spawn(|| {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let engine = rt.block_on(start(&[]));
        let pid = engine.pid().expect("engine runs");
        std::mem::forget(engine);
        pid
    })
    .join()
    .unwrap();
    // Nobody reaps it, so it stays as a zombie.
    let state = || std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !state().contains(") Z ") {
        assert!(std::time::Instant::now() < deadline, "engine still running: {}", state());
        std::thread::sleep(Duration::from_millis(10));
    }
}
