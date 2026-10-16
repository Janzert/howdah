//! The daemon over HTTP: the server and client together.

use std::path::PathBuf;
use std::time::Duration;

use howdah_arimaa::Color;
use howdah_tourney::client::{Client, ClientError};
use howdah_tourney::{Daemon, DaemonEvent, DaemonInfo, EngineLine, INFO_FILE, TestSpec, TestStatus, server};

fn test_engine() -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    let dir = exe.parent().and_then(|d| d.parent()).unwrap();
    let program = dir.join(if cfg!(windows) { "aei-test-engine.exe" } else { "aei-test-engine" });
    assert!(program.exists(), "run `cargo build -p howdah-aei --bin aei-test-engine`");
    program
}

fn spec() -> TestSpec {
    let program = test_engine().display().to_string().replace('\\', "/");
    TestSpec::parse(&format!(
        "name = \"http\"\ntime_control = \"0s/30s/100/0/12t\"\nmax_pairs = 2\nconcurrency = 2\n\
         [[engines]]\nname = \"one\"\nprogram = \"{program}\"\n\
         [[engines]]\nname = \"two\"\nprogram = \"{program}\"\n"
    ))
    .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_test_through_the_http_api() {
    let tmp = tempfile::tempdir().unwrap();
    let daemon = Daemon::start(tmp.path(), 0).unwrap();
    let (listener, info) = server::bind(tmp.path(), 0).await.unwrap();
    assert_eq!(DaemonInfo::read(tmp.path()), Some(info.clone()));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(tmp.path().join(INFO_FILE)).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    let serving = tokio::spawn(server::serve(daemon, listener, info));

    let client = Client::connect(tmp.path()).unwrap();
    let mut events = client.events().await.unwrap();
    let test = client.submit(&spec(), 3).await.unwrap();
    assert_eq!((test.id.as_str(), test.priority, test.status), ("0001-http", 3, TestStatus::Active));
    client.set_slots(2).await.unwrap();

    let mut finished = 0;
    let wait = tokio::time::timeout(Duration::from_secs(60), async {
        while let Some(e) = events.next().await {
            match e.unwrap() {
                DaemonEvent::GameFinished { .. } => finished += 1,
                DaemonEvent::TestChanged { test }
                    if test.status == TestStatus::Finished && test.running.is_empty() =>
                {
                    break;
                }
                _ => {}
            }
        }
    });
    wait.await.expect("the test finished");
    assert_eq!(finished, 4);

    let tests = client.list().await.unwrap();
    assert_eq!(tests.len(), 1);
    assert_eq!(tests[0].summary.games, 4);
    let games = client.games(&test.id).await.unwrap();
    assert_eq!(games.len(), 4);
    let record = client.record(&test.id, games[0].pair, games[0].game).await.unwrap();
    assert!(record.contains("[Event \"http\"]"), "{record}");

    let err = client.info("0009-nope").await.unwrap_err();
    assert!(matches!(&err, ClientError::Daemon(m) if m.contains("no test")), "{err}");
    let err = client.pause(&test.id).await.unwrap_err();
    assert!(err.to_string().contains("isn't active"), "{err}");
    let mut relative = spec();
    relative.engines[0].program = "bots/one".into();
    assert!(client.submit(&relative, 0).await.unwrap_err().to_string().contains("absolute path"));

    client.set_slots(0).await.unwrap();
    let second = client.submit(&spec(), 0).await.unwrap();
    let err = client.delete(&second.id).await.unwrap_err();
    assert!(err.to_string().contains("is active"), "{err}");
    client.stop(&second.id).await.unwrap();
    client.delete(&second.id).await.unwrap();
    let zip = client.archive(&test.id).await.unwrap();
    assert_eq!(zip, tmp.path().join("archive").join(format!("{}.zip", test.id)));
    assert!(zip.is_file() && !test.path.exists() && !second.path.exists());
    assert!(client.list().await.unwrap().is_empty());

    let mut wrong = DaemonInfo::read(tmp.path()).unwrap();
    wrong.token = "nope".into();
    let err = Client::new(&wrong).status().await.unwrap_err();
    assert!(err.to_string().contains("token"), "{err}");

    let status = client.shutdown(false).await.unwrap();
    assert!(status.closing);
    tokio::time::timeout(Duration::from_secs(30), serving).await.unwrap().unwrap().unwrap();
    assert!(!tmp.path().join(INFO_FILE).exists());
    assert!(matches!(Client::connect(tmp.path()), Err(ClientError::NotRunning(_))));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_running_games_engine_output() {
    let tmp = tempfile::tempdir().unwrap();
    let daemon = Daemon::start(tmp.path(), 0).unwrap();
    let (listener, info) = server::bind(tmp.path(), 0).await.unwrap();
    let serving = tokio::spawn(server::serve(daemon, listener, info));
    let client = Client::connect(tmp.path()).unwrap();
    let mut events = client.events().await.unwrap();
    let program = test_engine().display().to_string().replace('\\', "/");
    let spec = TestSpec::parse(&format!(
        "name = \"output\"\ntime_control = \"0s/30s/100/0/6t\"\nmax_pairs = 1\nconcurrency = 1\n\
         [[engines]]\nname = \"one\"\nprogram = \"{program}\"\nargs = [\"--think-ms\", \"200\", \"--name\", \"Uno\"]\n\
         [[engines]]\nname = \"two\"\nprogram = \"{program}\"\nargs = [\"--think-ms\", \"200\"]\n"
    ))
    .unwrap();
    let test = client.submit(&spec, 0).await.unwrap();
    client.set_slots(1).await.unwrap();

    let wait = tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            if let DaemonEvent::GameStarted { pair, game, .. } = events.next().await.unwrap().unwrap() {
                break (pair, game);
            }
        }
    });
    let (pair, game) = wait.await.expect("a game started");
    let mut output = client.output(&test.id, pair, game).await.unwrap();
    let line = tokio::time::timeout(Duration::from_secs(30), output.next()).await.expect("output came");
    let line = line.unwrap().unwrap();
    assert_eq!((line.test.as_str(), line.pair, line.game), (test.id.as_str(), pair, game));
    assert!(matches!(&line.line, EngineLine::Info(_) | EngineLine::Log(_)), "{line:?}");

    let live = client.live(&test.id, pair, game).await.unwrap();
    // Game 1 has engine one as gold.
    assert_eq!(live.engine_ids, [Some("Uno".into()), Some("aei-test-engine".into())]);
    let side = if line.side == Color::Gold { 0 } else { 1 };
    assert!(live.output[side].contains(&line.line), "{live:?}");

    client.shutdown(true).await.unwrap();
    tokio::time::timeout(Duration::from_secs(30), serving).await.unwrap().unwrap().unwrap();
}
