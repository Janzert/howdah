//! Running tests against `aei-test-engine` (built by
//! `cargo build -p howdah-aei --bin aei-test-engine`; `cargo test
//! --workspace` builds it).

use std::path::{Path, PathBuf};

use howdah_arimaa::GameRecord;
use howdah_tourney::{Decision, RunEnd, RunEvent, RunOptions, StopHandle, TestDir, TestSpec, run};

fn test_engine() -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    let dir = exe.parent().and_then(|d| d.parent()).unwrap();
    let program = dir.join(if cfg!(windows) { "aei-test-engine.exe" } else { "aei-test-engine" });
    assert!(
        program.exists(),
        "{} is missing: run `cargo build -p howdah-aei --bin aei-test-engine`",
        program.display()
    );
    program
}

/// A test of two random movers with fast clocks and a turn limit, so games
/// end quickly. `second_args` go to engine 2, and `second_extra` into its
/// table.
fn make_test(dir: &Path, extra: &str, second_args: &[&str], second_extra: &str) -> TestDir {
    let program = test_engine().display().to_string().replace('\\', "/");
    let args: Vec<String> = second_args.iter().map(|a| format!("{a:?}")).collect();
    let text = format!(
        "name = \"random\"\ntime_control = \"0s/30s/100/0/12t\"\n{extra}\n\
         [[engines]]\nname = \"one\"\nprogram = \"{program}\"\n\
         [[engines]]\nname = \"two\"\nprogram = \"{program}\"\nargs = [{}]\n{second_extra}\n",
        args.join(", ")
    );
    let spec = TestSpec::parse(&text).unwrap();
    TestDir::create(spec, Path::new("."), &dir.join("test")).unwrap()
}

async fn play(test: &TestDir, stop: StopHandle, stop_after: Option<usize>) -> (RunEnd, Vec<RunEvent>) {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut runner = Box::pin(run(test.clone(), RunOptions::default(), stop.clone(), tx));
    let mut events = Vec::new();
    let end = loop {
        tokio::select! {
            end = &mut runner => break end.unwrap(),
            Some(e) = rx.recv() => {
                if matches!(e, RunEvent::GameFinished { .. })
                    && stop_after.is_some_and(|n| n == events.iter().filter(|e| matches!(e, RunEvent::GameFinished { .. })).count() + 1)
                {
                    stop.stop();
                }
                events.push(e);
            }
        }
    };
    while let Ok(e) = rx.try_recv() {
        events.push(e);
    }
    (end, events)
}

#[tokio::test(flavor = "multi_thread")]
async fn plays_every_pair_and_records_the_games() {
    let tmp = tempfile::tempdir().unwrap();
    let test = make_test(tmp.path(), "max_pairs = 3\nconcurrency = 2", &[], "");
    let (end, events) = play(&test, StopHandle::default(), None).await;
    assert_eq!(end, RunEnd::MaxPairs);
    let finished = events.iter().filter(|e| matches!(e, RunEvent::GameFinished { .. })).count();
    assert_eq!(finished, 6);

    let mut entries = test.results().unwrap();
    entries.sort_by_key(|e| (e.pair, e.game));
    let games: Vec<(u32, u8)> = entries.iter().map(|e| (e.pair, e.game)).collect();
    assert_eq!(games, [(1, 1), (1, 2), (2, 1), (2, 2), (3, 1), (3, 2)]);
    for e in &entries {
        let record = GameRecord::parse(&std::fs::read_to_string(test.path.join(&e.record)).unwrap()).unwrap();
        let gold = if e.game == 1 { "one" } else { "two" };
        assert_eq!(record.tag("Gold"), Some(gold));
        assert_eq!(record.tag("Round"), Some(format!("{}.{}", e.pair, e.game).as_str()));
        assert_eq!(record.tag("Event"), Some("random"));
        let result = if e.winner == e.gold() { "1-0" } else { "0-1" };
        assert_eq!(record.tag("Result"), Some(result), "{e:?}");
        assert!(e.plies <= 24, "{e:?}");
        assert_eq!(record.tree.main_line().len() as u32, e.plies + 1);
    }

    // Everything is played: running again plays nothing.
    let (end, events) = play(&test, StopHandle::default(), None).await;
    assert_eq!(end, RunEnd::MaxPairs);
    assert!(events.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_stopped_test_resumes_where_it_left_off() {
    let tmp = tempfile::tempdir().unwrap();
    let test = make_test(tmp.path(), "max_pairs = 2", &[], "");
    let (end, _) = play(&test, StopHandle::default(), Some(1)).await;
    assert_eq!(end, RunEnd::Stopped);
    // The slot may take its next game before the stop arrives.
    let mut done: Vec<(u32, u8)> = test.results().unwrap().iter().map(|e| (e.pair, e.game)).collect();
    done.sort();
    assert!((1..4).contains(&done.len()), "{done:?}");

    let reopened = TestDir::open(&test.path).unwrap();
    let (end, events) = play(&reopened, StopHandle::default(), None).await;
    assert_eq!(end, RunEnd::MaxPairs);
    let started: Vec<(u32, u8)> = events
        .iter()
        .filter_map(|e| match e {
            RunEvent::GameStarted { pair, game } => Some((*pair, *game)),
            _ => None,
        })
        .collect();
    let all = [(1, 1), (1, 2), (2, 1), (2, 2)];
    assert_eq!(started, all[done.len()..]);
    assert_eq!(reopened.results().unwrap().len(), 4);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_engine_crashing_again_and_again_halts_the_test() {
    let tmp = tempfile::tempdir().unwrap();
    let test = make_test(tmp.path(), "max_pairs = 10", &["--mode", "crash"], "");
    let (end, events) = play(&test, StopHandle::default(), None).await;
    let RunEnd::Halted(why) = end else { panic!("{end:?}") };
    assert!(why.starts_with("two failed 3 times in a row"), "{why}");
    let restarts = events.iter().filter(|e| matches!(e, RunEvent::EngineRestarted { engine: 1, .. })).count();
    assert_eq!(restarts, 3);
    let entries = test.results().unwrap();
    assert_eq!(entries.len(), 3);
    assert!(entries.iter().all(|e| e.winner == 0 && e.reason == "forfeit"));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_sprt_ends_a_one_sided_test() {
    let tmp = tempfile::tempdir().unwrap();
    let test = make_test(
        tmp.path(),
        "concurrency = 2\n[sprt]\nelo0 = 0\nelo1 = 50",
        &["--mode", "resign"],
        "restart_each_game = true",
    );
    let (end, _) = play(&test, StopHandle::default(), None).await;
    assert_eq!(end, RunEnd::Decided(Decision::Pass));
    let entries = test.results().unwrap();
    assert!(entries.iter().all(|e| e.winner == 0 && e.reason == "resignation"));
    assert!(entries.len() < 40, "{} games", entries.len());

    // A decided test doesn't play on.
    let (end, events) = play(&test, StopHandle::default(), None).await;
    assert_eq!(end, RunEnd::Decided(Decision::Pass));
    assert!(events.is_empty());
}

async fn play_with(test: &TestDir, options: RunOptions) -> (RunEnd, Vec<RunEvent>) {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let end = run(test.clone(), options, StopHandle::default(), tx).await.unwrap();
    let mut events = Vec::new();
    while let Ok(e) = rx.try_recv() {
        events.push(e);
    }
    (end, events)
}

fn restarts_of(events: &[RunEvent], engine: usize) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| match e {
            RunEvent::EngineRestarted { engine: i, reason } if *i == engine => Some(reason.clone()),
            _ => None,
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn an_engine_that_keeps_growing_is_restarted_between_games() {
    if howdah_aei::process::resident_memory(std::process::id()).is_none() {
        eprintln!("resident memory can't be read here");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    // About 240 MB a game, so it passes the 256 MB floor in its second,
    // at about twice its size after the first.
    let test = make_test(tmp.path(), "max_pairs = 1\n[memory]\ngrowth = 1.5", &["--grow-mb", "20"], "");
    let (end, events) = play(&test, StopHandle::default(), None).await;
    assert_eq!(end, RunEnd::MaxPairs);
    let mut entries = test.results().unwrap();
    entries.sort_by_key(|e| e.game);
    let [first, second] = &entries[..] else { panic!("{entries:?}") };
    let [_, grown] = first.memory_mb.unwrap();
    assert!(grown >= 100, "{first:?}");
    assert!(first.restarts.is_empty(), "{first:?}");
    // Played on after the first game, restarted after the second.
    assert_eq!(second.restarts.len(), 1, "{second:?}");
    assert_eq!(second.restarts[0].engine, 1);
    assert!(second.restarts[0].reason.contains("after its first game"), "{second:?}");
    assert_eq!(restarts_of(&events, 1), [second.restarts[0].reason.clone()]);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_engine_over_its_limit_forfeits_mid_game() {
    if howdah_aei::process::resident_memory(std::process::id()).is_none() {
        eprintln!("resident memory can't be read here");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    // 100 MB a second: past the limit within a second, well before the
    // turn limit and the kernel's cap (356 MB).
    let test = make_test(
        tmp.path(),
        "max_pairs = 1\n[memory]\nlimit_mb = 100",
        &["--grow-mb", "20", "--think-ms", "200"],
        "",
    );
    let (end, events) = play(&test, StopHandle::default(), None).await;
    assert_eq!(end, RunEnd::MaxPairs);
    let entries = test.results().unwrap();
    assert_eq!(entries.len(), 2);
    for e in &entries {
        assert_eq!((e.winner, e.reason.as_str()), (0, howdah_tourney::RESOURCE_LIMIT), "{e:?}");
        let detail = e.detail.as_deref().unwrap();
        assert!(detail.starts_with("two used") && detail.contains("over its limit of 100 MB"), "{detail}");
        assert!(e.plies < 80, "{e:?}");
        assert_eq!(e.restarts.iter().map(|r| r.engine).collect::<Vec<_>>(), [1, 0]);
        // The record says so too.
        let record = GameRecord::parse(&std::fs::read_to_string(test.path.join(&e.record)).unwrap()).unwrap();
        assert_eq!(record.tag("Termination"), Some(detail));
        assert_eq!(record.tag("Result"), Some(if e.game == 1 { "1-0" } else { "0-1" }));
    }
    assert_eq!(restarts_of(&events, 1).len(), 2);
    let summary = howdah_tourney::summarize(None, &entries);
    assert_eq!(summary.reasons.get(howdah_tourney::RESOURCE_LIMIT), Some(&2));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_largest_engine_forfeits_when_all_are_over_the_budget() {
    if howdah_aei::process::resident_memory(std::process::id()).is_none() {
        eprintln!("resident memory can't be read here");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let test = make_test(tmp.path(), "max_pairs = 1", &["--grow-mb", "20", "--think-ms", "200"], "");
    let options = RunOptions { memory_budget: Some(120 << 20), ..RunOptions::default() };
    let (end, _) = play_with(&test, options).await;
    assert_eq!(end, RunEnd::MaxPairs);
    for e in test.results().unwrap() {
        assert_eq!((e.winner, e.reason.as_str()), (0, howdah_tourney::RESOURCE_LIMIT), "{e:?}");
        let detail = e.detail.unwrap();
        assert!(detail.contains("over the budget of 120 MB; two was the largest"), "{detail}");
    }
}
