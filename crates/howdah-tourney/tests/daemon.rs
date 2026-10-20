//! The daemon's queue, driven directly, with `aei-test-engine`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use howdah_tourney::{Daemon, DaemonEvent, RunEnd, TestInfo, TestSpec, TestStatus, TourneyError};
use tokio::sync::broadcast::Receiver;

fn test_engine() -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    let dir = exe.parent().and_then(|d| d.parent()).unwrap();
    let program = dir.join(if cfg!(windows) { "aei-test-engine.exe" } else { "aei-test-engine" });
    assert!(program.exists(), "run `cargo build -p howdah-aei --bin aei-test-engine`");
    program
}

fn spec(name: &str, extra: &str, second_args: &[&str]) -> TestSpec {
    let program = test_engine().display().to_string().replace('\\', "/");
    let args: Vec<String> = second_args.iter().map(|a| format!("{a:?}")).collect();
    TestSpec::parse(&format!(
        "name = \"{name}\"\ntime_control = \"0s/30s/100/0/12t\"\n{extra}\n\
         [[engines]]\nname = \"one\"\nprogram = \"{program}\"\n\
         [[engines]]\nname = \"two\"\nprogram = \"{program}\"\nargs = [{}]\n",
        args.join(", ")
    ))
    .unwrap()
}

/// Events until `done` says so (or a minute passes).
async fn events_until(
    rx: &mut Receiver<DaemonEvent>,
    mut done: impl FnMut(&DaemonEvent) -> bool,
) -> Vec<DaemonEvent> {
    let mut seen = Vec::new();
    tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            let e = rx.recv().await.unwrap();
            let stop = done(&e);
            seen.push(e);
            if stop {
                break;
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out; saw {seen:#?}"));
    seen
}

fn changed<'a>(e: &'a DaemonEvent, id: &str) -> Option<&'a TestInfo> {
    match e {
        DaemonEvent::TestChanged { test } if test.id == id => Some(test),
        _ => None,
    }
}

fn finished(e: &DaemonEvent, id: &str) -> bool {
    changed(e, id).is_some_and(|t| t.status == TestStatus::Finished && t.running.is_empty())
}

async fn close(daemon: &Daemon) {
    daemon.shutdown(false);
    tokio::time::timeout(Duration::from_secs(30), daemon.closed()).await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn higher_priority_tests_go_first() {
    let tmp = tempfile::tempdir().unwrap();
    let daemon = Daemon::start(tmp.path(), 0).unwrap();
    let low = daemon.submit(spec("low", "max_pairs = 2\nconcurrency = 2", &[]), 0).unwrap();
    let high = daemon.submit(spec("high", "max_pairs = 2\nconcurrency = 2", &[]), 5).unwrap();
    assert_eq!((low.id.as_str(), high.id.as_str()), ("0001-low", "0002-high"));
    let mut rx = daemon.subscribe();
    daemon.set_slots(2);

    let mut low_done = false;
    let mut high_done = false;
    let events = events_until(&mut rx, |e| {
        low_done |= finished(e, &low.id);
        high_done |= finished(e, &high.id);
        low_done && high_done
    })
    .await;
    let starts: Vec<&str> = events
        .iter()
        .filter_map(|e| match e {
            DaemonEvent::GameStarted { test, .. } => Some(test.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(starts.len(), 8, "{starts:?}");
    let last_high = starts.iter().rposition(|t| *t == high.id).unwrap();
    let first_low = starts.iter().position(|t| *t == low.id).unwrap();
    assert!(last_high < first_low, "{starts:?}");
    for t in daemon.list() {
        assert_eq!((t.status, t.end, t.summary.games), (TestStatus::Finished, Some(RunEnd::MaxPairs), 4));
    }
    close(&daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_paused_test_stays_paused_across_a_restart_and_resumes() {
    let tmp = tempfile::tempdir().unwrap();
    let daemon = Daemon::start(tmp.path(), 1).unwrap();
    let mut rx = daemon.subscribe();
    let t = daemon.submit(spec("p", "max_pairs = 3", &[]), 0).unwrap();
    events_until(&mut rx, |e| matches!(e, DaemonEvent::GameFinished { .. })).await;
    daemon.pause(&t.id).unwrap();
    close(&daemon).await;
    let played = daemon.games(&t.id).unwrap().len();
    assert!((1..6).contains(&played), "{played}");

    let daemon = Daemon::start(tmp.path(), 1).unwrap();
    let info = daemon.info(&t.id).unwrap();
    assert_eq!((info.status, info.summary.games as usize), (TestStatus::Paused, played));
    let mut rx = daemon.subscribe();
    // Nothing starts while it's paused.
    assert!(tokio::time::timeout(Duration::from_millis(300), rx.recv()).await.is_err());
    daemon.resume(&t.id).unwrap();
    events_until(&mut rx, |e| finished(e, &t.id)).await;
    let mut games: Vec<(u32, u8)> = daemon.games(&t.id).unwrap().iter().map(|g| (g.pair, g.game)).collect();
    games.sort();
    assert_eq!(games, [(1, 1), (1, 2), (2, 1), (2, 2), (3, 1), (3, 2)]);
    assert!(daemon.record(&t.id, 3, 2).unwrap().contains("[Round \"3.2\"]"));
    close(&daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_halted_test_can_be_resumed_and_a_finished_one_extended() {
    let tmp = tempfile::tempdir().unwrap();
    let daemon = Daemon::start(tmp.path(), 1).unwrap();
    let mut rx = daemon.subscribe();
    let bad = daemon.submit(spec("bad", "max_pairs = 5", &["--mode", "crash"]), 0).unwrap();
    events_until(&mut rx, |e| finished(e, &bad.id)).await;
    let info = daemon.info(&bad.id).unwrap();
    assert!(
        matches!(&info.end, Some(RunEnd::Halted(why)) if why.starts_with("two failed 3 times")),
        "{info:?}"
    );
    let resumed = daemon.resume(&bad.id).unwrap();
    assert_eq!(resumed.status, TestStatus::Active);
    events_until(&mut rx, |e| finished(e, &bad.id)).await;
    assert_eq!(daemon.games(&bad.id).unwrap().len(), 6);

    let ok = daemon.submit(spec("ok", "max_pairs = 1", &[]), 0).unwrap();
    events_until(&mut rx, |e| finished(e, &ok.id)).await;
    assert!(
        matches!(daemon.resume(&ok.id), Err(TourneyError::Refused(m)) if m.contains("raise its max pairs"))
    );
    let more = daemon.set_max_pairs(&ok.id, Some(2)).unwrap();
    assert_eq!(more.status, TestStatus::Active);
    events_until(&mut rx, |e| finished(e, &ok.id)).await;
    assert_eq!(daemon.games(&ok.id).unwrap().len(), 4);
    close(&daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn games_at_once_can_change_and_stay_changed() {
    let tmp = tempfile::tempdir().unwrap();
    let daemon = Daemon::start(tmp.path(), 2).unwrap();
    let t = daemon.submit(spec("c", "max_pairs = 3\nconcurrency = 1", &["--think-ms", "100"]), 0).unwrap();
    assert_eq!(t.concurrency, 1);
    assert!(daemon.set_concurrency(&t.id, 0).unwrap_err().to_string().contains("at least 1"));
    assert_eq!(daemon.set_concurrency(&t.id, 2).unwrap().concurrency, 2);
    // The second slot takes a game too.
    let both = tokio::time::timeout(Duration::from_secs(20), async {
        while daemon.info(&t.id).unwrap().running.len() < 2 {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    });
    both.await.expect("two games at once");
    daemon.pause(&t.id).unwrap();
    close(&daemon).await;

    let daemon = Daemon::start(tmp.path(), 0).unwrap();
    assert_eq!(daemon.info(&t.id).unwrap().concurrency, 2, "kept in state.json");
    close(&daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_shutdown_can_be_called_off() {
    let tmp = tempfile::tempdir().unwrap();
    let daemon = Daemon::start(tmp.path(), 0).unwrap();
    let t = daemon.submit(spec("t", "max_pairs = 2", &[]), 0).unwrap();
    let mut rx = daemon.subscribe();
    daemon.set_slots(1);
    events_until(&mut rx, |e| matches!(e, DaemonEvent::GameStarted { .. })).await;
    daemon.shutdown(false);
    assert!(daemon.status().closing);
    daemon.cancel_shutdown().unwrap();
    assert!(!daemon.status().closing);
    // The slot goes on to the test's other games, whether it had already
    // decided to stop or not.
    events_until(&mut rx, |e| finished(e, &t.id)).await;
    assert_eq!(daemon.info(&t.id).unwrap().summary.games, 4);

    // Once the games are abandoned, or the daemon is done, it's too late.
    daemon.shutdown(true);
    assert!(daemon.cancel_shutdown().is_err());
    daemon.closed().await;
    assert!(daemon.cancel_shutdown().is_err());
}

#[tokio::test(flavor = "multi_thread")]
async fn refusals() {
    let tmp = tempfile::tempdir().unwrap();
    let daemon = Daemon::start(tmp.path(), 0).unwrap();
    let mut relative = spec("rel", "max_pairs = 1", &[]);
    relative.engines[1].program = Path::new("bots/two").into();
    let err = daemon.submit(relative, 0).unwrap_err();
    assert!(err.to_string().contains("absolute path"), "{err}");
    assert!(matches!(daemon.info("nope"), Err(TourneyError::NoTest(_))));
    let t = daemon.submit(spec("t", "max_pairs = 1", &[]), 0).unwrap();
    assert!(daemon.resume(&t.id).is_err());
    assert!(daemon.set_max_pairs(&t.id, None).unwrap_err().to_string().contains("needs a pair limit"));
    daemon.stop(&t.id).unwrap();
    assert_eq!(daemon.info(&t.id).unwrap().end, Some(RunEnd::Stopped));
    daemon.shutdown(false);
    assert!(daemon.submit(spec("late", "max_pairs = 1", &[]), 0).is_err());
    daemon.closed().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_viewer_can_join_a_running_game() {
    let tmp = tempfile::tempdir().unwrap();
    let daemon = Daemon::start(tmp.path(), 0).unwrap();
    // Engine two takes its time over its sixth move, so the game is still
    // running when the viewer joins after its fifth.
    let t = daemon
        .submit(spec("live", "max_pairs = 1", &["--mode", "slow", "--delay-ms", "300", "--after", "5"]), 0)
        .unwrap();
    let mut rx = daemon.subscribe();
    daemon.set_slots(1);
    let mut seen: Vec<String> = Vec::new();
    let mut joined = None;
    events_until(&mut rx, |e| {
        if let DaemonEvent::MovePlayed { test, pair: 1, game: 1, ply, notation, .. } = e {
            assert_eq!(test, &t.id);
            assert_eq!(*ply, seen.len());
            seen.push(notation.clone());
            if *ply == 9 {
                joined = Some(daemon.live(&t.id, 1, 1).unwrap());
            }
        }
        matches!(e, DaemonEvent::GameFinished { entry, .. } if entry.game == 1)
    })
    .await;
    let live = joined.expect("joined the game");
    assert_eq!((live.gold.as_str(), live.silver.as_str()), ("one", "two"));
    assert_eq!(live.moves, seen[..10]);
    assert_eq!(live.used_ms.len(), 10);
    assert!(live.reserves_ms.is_some());
    // A finished game isn't live any more; its record is there instead.
    assert!(daemon.live(&t.id, 1, 1).is_err());
    let record = daemon.record(&t.id, 1, 1).unwrap();
    assert!(record.contains(&seen[9]));
    let entry = &daemon.games(&t.id).unwrap()[0];
    assert!(entry.result().is_some());
    close(&daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn idle_slots_let_their_engines_go() {
    let tmp = tempfile::tempdir().unwrap();
    let daemon = Daemon::start(tmp.path(), 0).unwrap();
    let mut rx = daemon.subscribe();
    let t = daemon.submit(spec("idle", "max_pairs = 2", &[]), 0).unwrap();
    daemon.set_slots(3);
    events_until(&mut rx, |e| finished(e, &t.id)).await;
    // Nothing more to play: within the slots' idle time (10 s), none of
    // them holds an engine.
    let mut engines = daemon.status().engines;
    for _ in 0..30 {
        if engines == 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
        engines = daemon.status().engines;
    }
    assert_eq!(engines, 0);
    close(&daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn finished_tests_can_be_archived_or_deleted() {
    let tmp = tempfile::tempdir().unwrap();
    let daemon = Daemon::start(tmp.path(), 0).unwrap();
    let mut rx = daemon.subscribe();
    let kept = daemon.submit(spec("kept", "max_pairs = 1", &[]), 0).unwrap();
    let gone = daemon.submit(spec("gone", "max_pairs = 1", &[]), 0).unwrap();
    let err = daemon.remove(&kept.id, true).await.unwrap_err();
    assert!(err.to_string().contains("is active"), "{err}");
    daemon.set_slots(1);
    let (mut a, mut b) = (false, false);
    events_until(&mut rx, |e| {
        a |= finished(e, &kept.id);
        b |= finished(e, &gone.id);
        a && b
    })
    .await;

    // The slot still holds the last test's engines; taking it out lets
    // them go at once rather than after the idle time.
    let zip = daemon.remove(&kept.id, true).await.unwrap().unwrap();
    assert!(daemon.remove(&gone.id, false).await.unwrap().is_none());
    assert_eq!(daemon.status().engines, 0);
    events_until(
        &mut rx,
        |e| matches!(e, DaemonEvent::TestRemoved { test, archive: None } if *test == gone.id),
    )
    .await;
    assert!(daemon.list().is_empty());
    assert!(!kept.path.exists() && !gone.path.exists());
    assert_eq!(zip, tmp.path().join("archive/0001-kept.zip"));
    let mut archive = zip::ZipArchive::new(std::fs::File::open(&zip).unwrap()).unwrap();
    let names: Vec<String> = archive.file_names().map(str::to_string).collect();
    for name in [
        "0001-kept/test.toml",
        "0001-kept/results.jsonl",
        "0001-kept/state.json",
        "0001-kept/games/0001-2.txt",
    ] {
        assert!(names.iter().any(|n| n == name), "{name} not in {names:?}");
    }
    let mut results = String::new();
    std::io::Read::read_to_string(&mut archive.by_name("0001-kept/results.jsonl").unwrap(), &mut results)
        .unwrap();
    assert_eq!(results.lines().count(), 2);
    assert!(matches!(daemon.remove(&kept.id, false).await, Err(TourneyError::NoTest(_))));

    // An archived test's number isn't used again.
    let next = daemon.submit(spec("next", "max_pairs = 1", &[]), 0).unwrap();
    assert_eq!(next.id, "0002-next");

    let archived = daemon.archived();
    assert_eq!(archived.len(), 1);
    assert_eq!(archived[0].file, "0001-kept.zip");
    let was = archived[0].test.as_ref().unwrap();
    assert_eq!((was.name.as_str(), was.status, was.summary.games), ("kept", TestStatus::Finished, 2));
    assert_eq!(was.end, Some(RunEnd::MaxPairs));
    for bad in ["../0001-kept.zip", ".zip", "nope.zip", "0001-kept"] {
        assert!(matches!(daemon.delete_archived(bad), Err(TourneyError::NoTest(_))), "{bad}");
    }

    // Restored, it's as it was, and its zip is gone.
    let back = daemon.restore("0001-kept.zip").unwrap();
    assert_eq!((back.id.as_str(), back.status, back.summary.games), ("0001-kept", TestStatus::Finished, 2));
    assert!(daemon.archived().is_empty() && !zip.exists());
    assert_eq!(daemon.games(&back.id).unwrap().len(), 2);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let program = back.path.join("engines/one").read_dir().unwrap().next().unwrap().unwrap().path();
        assert_ne!(std::fs::metadata(program).unwrap().permissions().mode() & 0o111, 0);
    }

    // Archived again, it can't be restored over itself, and can be deleted.
    let zip = daemon.remove(&back.id, true).await.unwrap().unwrap();
    std::fs::create_dir(tmp.path().join("tests/0001-kept")).unwrap();
    let err = daemon.restore("0001-kept.zip").unwrap_err();
    assert!(err.to_string().contains("in the queue already"), "{err}");
    std::fs::remove_dir(tmp.path().join("tests/0001-kept")).unwrap();
    daemon.delete_archived("0001-kept.zip").unwrap();
    assert!(daemon.archived().is_empty() && !zip.exists());
    close(&daemon).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_undecided_test_carries_on_when_its_limit_rises() {
    let tmp = tempfile::tempdir().unwrap();
    let daemon = Daemon::start(tmp.path(), 0).unwrap();
    // As in run.rs: 3 pairs can't decide, so it ends after the first.
    let extra = "max_pairs = 3\n[sprt]\nelo0 = 0\nelo1 = 10\nundecided_below = 0.6";
    let t = daemon.submit(spec("short", extra, &[]), 0).unwrap();
    let mut rx = daemon.subscribe();
    daemon.set_slots(1);
    events_until(&mut rx, |e| finished(e, &t.id)).await;
    let info = daemon.info(&t.id).unwrap();
    assert_eq!((info.end, info.summary.games), (Some(RunEnd::Undecided), 2));

    // It can't be resumed as it is, but with room to decide it carries on.
    assert!(matches!(daemon.resume(&t.id), Err(TourneyError::Refused(_))));
    let info = daemon.set_max_pairs(&t.id, Some(3000)).unwrap();
    assert!(info.decide_chance.unwrap() > 0.6, "{:?}", info.decide_chance);
    assert_eq!((info.status, info.end), (TestStatus::Active, None));
    let p = info.projection.expect("a running SPRT test has a projection");
    assert!((p.pass + p.fail + p.undecided + p.done - 1.0).abs() < 1e-9, "{p:?}");

    // A cutoff above its chance ends it again (once running games finish).
    let mut rx = daemon.subscribe();
    let info = daemon.set_undecided_below(&t.id, Some(0.99)).unwrap();
    assert_eq!(info.undecided_below, Some(0.99));
    if info.status != TestStatus::Finished {
        events_until(&mut rx, |e| finished(e, &t.id)).await;
    }
    let info = daemon.info(&t.id).unwrap();
    assert_eq!((info.end, info.projection), (Some(RunEnd::Undecided), None));
    assert!(matches!(daemon.set_undecided_below(&t.id, Some(1.5)), Err(TourneyError::Refused(_))));

    // With the cutoff off it carries on, and stays off after a restart.
    let info = daemon.set_undecided_below(&t.id, None).unwrap();
    assert_eq!((info.status, info.undecided_below), (TestStatus::Active, None));
    daemon.stop(&t.id).unwrap();
    close(&daemon).await;
    let daemon = Daemon::start(tmp.path(), 0).unwrap();
    assert_eq!(daemon.info(&t.id).unwrap().undecided_below, None);
    close(&daemon).await;
}
