//! Running tests against `aei-test-engine` (built by
//! `cargo build -p howdah-aei --bin aei-test-engine`; `cargo test
//! --workspace` builds it).

use std::path::{Path, PathBuf};

use howdah_arimaa::GameRecord;
use howdah_tourney::{
    Decision, RunEnd, RunEvent, RunOptions, SPSA_HISTORY_FILE, SPSA_POINTS_FILE, SpsaPoint, SpsaStep,
    StopHandle, TestDir, TestSpec, run, spsa_step,
};

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
        assert_eq!(record.tag("SetupMinimum"), Some("0"), "engines get no setup minute");
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
async fn a_test_unlikely_to_decide_ends_undecided() {
    // With 3 pairs the SPRT can't decide, and after any first pair the
    // projected chance of deciding is under 0.6 (0.57 at most, after a
    // pair won or lost by both), so the test ends after one pair.
    let tmp = tempfile::tempdir().unwrap();
    let test =
        make_test(tmp.path(), "max_pairs = 3\n[sprt]\nelo0 = 0\nelo1 = 10\nundecided_below = 0.6", &[], "");
    let (end, _) = play(&test, StopHandle::default(), None).await;
    assert_eq!(end, RunEnd::Undecided);
    assert_eq!(test.results().unwrap().len(), 2);
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
            RunEvent::EngineRestarted { engine: i, reason, .. } if *i == engine => Some(reason.clone()),
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

/// An SPSA run of the random mover: `iterations` of one pair each.
fn make_spsa(dir: &Path, iterations: u32) -> TestDir {
    let program = test_engine().display().to_string().replace('\\', "/");
    let text = format!(
        "name = \"tune\"\ntime_control = \"0s/30s/100/0/12t\"\nconcurrency = 2\nengine_logs = true\n\
         [spsa]\niterations = {iterations}\npairs_per_iteration = 1\nseed = 3\n\
         [[spsa.params]]\nname = \"weight\"\nstart = 1.0\nmin = 0.0\nmax = 2.0\nc_end = 0.1\nr_end = 0.5\n\
         [[spsa.params]]\nname = \"depth\"\nstart = 5\nmin = 1\nmax = 9\nint = true\nc_end = 1\n\
         [[engines]]\nname = \"rand\"\nprogram = \"{program}\"\noptions = {{ hash = 16 }}\n"
    );
    let spec = TestSpec::parse(&text).unwrap();
    TestDir::create(spec, Path::new("."), &dir.join("test")).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn an_spsa_run_steps_its_parameters_and_resumes() {
    let tmp = tempfile::tempdir().unwrap();
    let test = make_spsa(tmp.path(), 3);
    let spsa = test.spec.spsa.clone().unwrap();
    let (end, _) = play(&test, StopHandle::default(), Some(2)).await;
    assert_eq!(end, RunEnd::Stopped);
    let (end, _) = play(&test, StopHandle::default(), None).await;
    assert_eq!(end, RunEnd::MaxPairs);

    let mut entries = test.results().unwrap();
    entries.sort_by_key(|e| (e.pair, e.game));
    assert_eq!(entries.len(), 6);
    let read = |name: &str| -> Vec<serde_json::Value> {
        let text = std::fs::read_to_string(test.path.join(name)).unwrap();
        text.lines().map(|l| serde_json::from_str(l).unwrap()).collect()
    };
    let points: Vec<SpsaPoint> =
        read(SPSA_POINTS_FILE).into_iter().map(|v| serde_json::from_value(v).unwrap()).collect();
    let steps: Vec<SpsaStep> =
        read(SPSA_HISTORY_FILE).into_iter().map(|v| serde_json::from_value(v).unwrap()).collect();
    assert_eq!(points.iter().map(|p| p.iteration).collect::<Vec<_>>(), [1, 2, 3]);
    let mut done: Vec<u32> = steps.iter().map(|s| s.iteration).collect();
    done.sort();
    assert_eq!(done, [1, 2, 3]);

    // θ follows the games: replaying the steps in the order they were taken
    // gives the history's last θ.
    let mut theta = vec![1.0, 5.0];
    for s in &steps {
        let point = &points[s.iteration as usize - 1];
        let games: Vec<_> = entries.iter().filter(|e| e.pair == s.iteration).collect();
        let wins = games.iter().filter(|e| e.winner == 0).count() as u32;
        assert_eq!((s.wins, s.losses), (wins, 2 - wins));
        spsa_step(&spsa, &mut theta, point, i64::from(wins) * 2 - 2);
        assert_eq!(s.theta["weight"], theta[0]);
    }
    let progress = howdah_tourney::spsa_progress(&test.path, &spsa).unwrap();
    assert_eq!(progress.done, 3);
    assert_eq!(progress.params[0].value, theta[0]);

    // Each side was sent its point's values, over the spec's options (its
    // log is named for `rand+`, made safe for a file name).
    for (pair, point) in points.iter().enumerate() {
        let pair = pair as u32 + 1;
        for (side, values) in [("rand_", &point.plus), ("rand-", &point.minus)] {
            let log = std::fs::read_to_string(test.path.join(format!("logs/{pair:04}-1-{side}.log")))
                .unwrap_or_else(|_| panic!("no log for {pair} {side}"));
            assert!(log.contains(&format!("> setoption name weight value {}", values[0])), "{log}");
            assert!(log.contains(&format!("> setoption name depth value {}", values[1])), "{log}");
        }
        assert_eq!(point.plus[1].fract(), 0.0);
    }
    let record =
        GameRecord::parse(&std::fs::read_to_string(test.path.join(&entries[1].record)).unwrap()).unwrap();
    assert_eq!((record.tag("Gold"), record.tag("Silver")), (Some("rand-"), Some("rand+")));
}

/// A pool of random movers, `extra` going into its `[pool]`, and each
/// engine `(name, args)`.
fn make_pool(dir: &Path, top: &str, pool: &str, engines: &[(&str, &[&str])]) -> TestDir {
    let program = test_engine().display().to_string().replace('\\', "/");
    let mut text = format!("name = \"pool\"\ntime_control = \"0s/30s/100/0/12t\"\n{top}\n[pool]\n{pool}\n");
    for (name, args) in engines {
        let args: Vec<String> = args.iter().map(|a| format!("{a:?}")).collect();
        text += &format!(
            "[[engines]]\nname = \"{name}\"\nprogram = \"{program}\"\nargs = [{}]\n",
            args.join(", ")
        );
    }
    let spec = TestSpec::parse(&text).unwrap();
    TestDir::create(spec, Path::new("."), &dir.join("pool")).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pool_covers_every_matchup_and_logs_each_pairs_engines() {
    let tmp = tempfile::tempdir().unwrap();
    let engines: [(&str, &[&str]); 3] = [("a", &[]), ("b", &[]), ("c", &[])];
    let test = make_pool(tmp.path(), "max_pairs = 6\nconcurrency = 2", "min_pairs = 2", &engines);
    let (end, _) = play(&test, StopHandle::default(), None).await;
    assert_eq!(end, RunEnd::MaxPairs);
    let entries = test.results().unwrap();
    assert_eq!(entries.len(), 12);
    let mut matchups = std::collections::HashMap::new();
    for e in &entries {
        let [x, y] = e.engines.expect("a pool logs each pair's engines");
        assert_ne!(x, y);
        let other = entries.iter().find(|o| o.pair == e.pair && o.game != e.game).unwrap();
        assert_eq!(other.engines, e.engines, "both games of a pair have the same engines");
        if e.game == 1 {
            *matchups.entry((x.min(y), x.max(y))).or_insert(0) += 1;
        }
        let record = GameRecord::parse(&std::fs::read_to_string(test.path.join(&e.record)).unwrap()).unwrap();
        let names = ["a", "b", "c"];
        let gold = if e.game == 1 { names[x] } else { names[y] };
        assert_eq!(record.tag("Gold"), Some(gold));
    }
    // min_pairs 2: two pairs of each of the three matchups.
    assert_eq!(matchups.values().copied().collect::<Vec<_>>(), [2, 2, 2]);
    let info = howdah_tourney::pool_info(test.spec.pool.as_ref().unwrap(), &test.spec.engines, &entries);
    assert_eq!(info.engines.iter().map(|e| e.games).sum::<u32>(), 24);
    assert_eq!(info.matchups.iter().map(|m| m.pairs).sum::<u32>(), 6);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pool_retires_an_engine_that_keeps_failing() {
    let tmp = tempfile::tempdir().unwrap();
    let engines: [(&str, &[&str]); 3] = [("a", &[]), ("b", &[]), ("broken", &["--mode", "crash"])];
    let test = make_pool(tmp.path(), "max_pairs = 8", "min_pairs = 1", &engines);
    let (end, events) = play(&test, StopHandle::default(), None).await;
    assert_eq!(end, RunEnd::MaxPairs, "{events:?}");
    let retired: Vec<&str> = events
        .iter()
        .filter_map(|e| match e {
            RunEvent::EngineRetired { name, .. } => Some(name.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(retired, ["broken"]);
    // Retiring it is kept in the spec.
    let reopened = TestDir::open(&test.path).unwrap();
    assert!(reopened.spec.engines[2].retired);
    // Once it's retired, a and b play on.
    let entries = reopened.results().unwrap();
    let last = entries.iter().max_by_key(|e| e.pair).unwrap();
    assert_eq!(last.engines.map(|[x, y]| x.max(y)), Some(1), "{entries:?}");
}
