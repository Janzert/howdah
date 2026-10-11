//! The daemon over HTTP: the server and client together.

use std::path::PathBuf;
use std::time::Duration;

use howdah_tourney::client::{Client, ClientError};
use howdah_tourney::{Daemon, DaemonEvent, DaemonInfo, INFO_FILE, TestSpec, TestStatus, server};

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
