//! Engine tests: the app as a client of howdah-tourney's daemon (plan in
//! `docs/TOURNAMENTS.md`). The lobby lists the daemon's tests and changes
//! them; the daemon's events reach it as `tourney://changed` while the app
//! is connected (it looks for a daemon every few seconds). A test's game
//! opens in a game window: a finished one as its record, a running one
//! followed like a watched arimaa.com game, between two remote players
//! whose moves and clocks come from the daemon.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use howdah_arimaa::{Color, GameRecord, TimeControl};
use howdah_tourney::client::{Client, ClientError, EventStream};
use howdah_tourney::{
    DaemonEvent, DaemonInfo, Decision, GameEntry, OptionValue, RunEnd, SprtSpec, TestInfo, TestSpec,
    TestStatus,
};

use crate::backend::{Events, emit, emit_session};
use crate::controller::{Controller, SharedSession};
use crate::dto::{
    ApiError, EngineSpec, NewTestSpec, SprtView, TestDetailView, TestGameRef, TestGameView, TestView,
    TourneyStatusView, TourneyUpdate,
};
use crate::session::{Player, RemoteClock, Session};

pub const TOURNEY_CHANGED: &str = "tourney://changed";

/// How often to look for a daemon while not connected.
const RECONNECT: Duration = Duration::from_secs(3);

/// The program that runs the daemon, looked for beside the app.
const DAEMON_PROGRAM: &str = if cfg!(windows) { "howdah-tourney.exe" } else { "howdah-tourney" };

pub struct Tourney {
    /// The daemon's data directory (None: none could be found).
    data_dir: Option<PathBuf>,
    events: Events,
    /// Whether the task passing on the daemon's events has started.
    forwarding: AtomicBool,
}

/// Where a test's game went.
pub enum Opened {
    /// It's running; the session follows it.
    Following,
    /// It's finished; this is its record.
    Finished(GameRecord),
}

fn api(e: ClientError) -> ApiError {
    ApiError::state(e)
}

fn lock(session: &SharedSession) -> std::sync::MutexGuard<'_, Session> {
    session.lock().unwrap_or_else(|e| e.into_inner())
}

impl Tourney {
    pub fn new(data_dir: Option<PathBuf>, events: Events) -> Tourney {
        Tourney { data_dir, events, forwarding: AtomicBool::new(false) }
    }

    fn data_dir(&self) -> Result<&Path, ApiError> {
        self.data_dir.as_deref().ok_or_else(|| ApiError::state("no data directory for engine tests"))
    }

    fn client(&self) -> Result<Client, ApiError> {
        Client::connect(self.data_dir()?).map_err(api)
    }

    /// The daemon's state. The first call starts passing on its events.
    pub async fn status(self: &Arc<Self>) -> TourneyStatusView {
        if self.data_dir.is_some() {
            self.forward();
        }
        let can_start = daemon_program().is_some() && self.data_dir.is_some();
        let data_dir = self.data_dir.as_ref().map(|d| d.display().to_string());
        let status = match self.client() {
            Ok(c) => c.status().await.ok(),
            Err(_) => None,
        };
        TourneyStatusView {
            connected: status.is_some(),
            data_dir,
            slots: status.as_ref().map(|s| s.slots),
            running: status.as_ref().map_or(0, |s| s.running.len() as u32),
            can_start,
        }
    }

    pub async fn tests(&self) -> Result<Vec<TestView>, ApiError> {
        Ok(self.client()?.list().await.map_err(api)?.iter().map(test_view).collect())
    }

    pub async fn test(&self, id: &str) -> Result<TestDetailView, ApiError> {
        let client = self.client()?;
        let info = client.info(id).await.map_err(api)?;
        let mut games: Vec<TestGameView> =
            client.games(id).await.map_err(api)?.iter().map(|g| game_view(id, &info.engines, g)).collect();
        games.reverse();
        Ok(TestDetailView { test: test_view(&info), games })
    }

    /// Adds a test of two engines from the engine list.
    pub async fn submit(&self, new: NewTestSpec, engines: [EngineSpec; 2]) -> Result<TestView, ApiError> {
        let spec = test_spec(new.clone(), engines)?;
        Ok(test_view(&self.client()?.submit(&spec, new.priority).await.map_err(api)?))
    }

    /// Pauses, resumes or stops a test.
    pub async fn control(&self, id: &str, action: &str) -> Result<TestView, ApiError> {
        let client = self.client()?;
        let info = match action {
            "pause" => client.pause(id).await,
            "resume" => client.resume(id).await,
            "stop" => client.stop(id).await,
            other => return Err(ApiError::state(format!("no test action {other:?}"))),
        };
        Ok(test_view(&info.map_err(api)?))
    }

    pub async fn set_priority(&self, id: &str, priority: i32) -> Result<TestView, ApiError> {
        Ok(test_view(&self.client()?.set_priority(id, priority).await.map_err(api)?))
    }

    pub async fn set_max_pairs(&self, id: &str, max_pairs: Option<u32>) -> Result<TestView, ApiError> {
        Ok(test_view(&self.client()?.set_max_pairs(id, max_pairs).await.map_err(api)?))
    }

    pub async fn set_slots(&self, slots: u32) -> Result<(), ApiError> {
        self.client()?.set_slots(slots).await.map_err(api).map(drop)
    }

    pub async fn shutdown(&self, now: bool) -> Result<(), ApiError> {
        self.client()?.shutdown(now).await.map_err(api).map(drop)
    }

    /// Starts a daemon with `slots` slots, from the `howdah-tourney`
    /// beside the app, and waits for it to answer. It keeps running after
    /// the app quits; its output goes to `daemon.log` in its directory.
    pub async fn start_daemon(&self, slots: u32) -> Result<(), ApiError> {
        let dir = self.data_dir()?.to_path_buf();
        if let Ok(c) = self.client()
            && c.status().await.is_ok()
        {
            return Ok(());
        }
        let program = daemon_program().ok_or_else(|| {
            ApiError::state(format!(
                "{DAEMON_PROGRAM} isn't beside Howdah; start it with `howdah-tourney daemon`"
            ))
        })?;
        std::fs::create_dir_all(&dir).map_err(|e| ApiError::state(format!("{}: {e}", dir.display())))?;
        let needs_copy = cfg!(windows) || std::env::var_os("APPIMAGE").is_some();
        let program = if needs_copy { runnable_copy(&program, &dir)? } else { program };
        let log_path = dir.join("daemon.log");
        let log = std::fs::File::create(&log_path)
            .map_err(|e| ApiError::state(format!("{}: {e}", log_path.display())))?;
        let err_log = log.try_clone().map_err(ApiError::state)?;
        let mut cmd = std::process::Command::new(&program);
        cmd.arg("--data-dir").arg(&dir).args(["daemon", "--slots", &slots.to_string()]);
        cmd.stdin(std::process::Stdio::null()).stdout(log).stderr(err_log);
        // Out of the app's process group (and console), so it outlives
        // the app and a Ctrl-C meant for it.
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
        #[cfg(windows)]
        std::os::windows::process::CommandExt::creation_flags(&mut cmd, 0x0000_0200 | 0x0800_0000);
        let mut child =
            cmd.spawn().map_err(|e| ApiError::state(format!("couldn't start {}: {e}", program.display())))?;
        // Reap it when it exits, so it doesn't linger as a zombie.
        std::thread::spawn(move || child.wait());
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(200)).await;
            if let Some(info) = DaemonInfo::read(&dir)
                && Client::new(&info).status().await.is_ok()
            {
                return Ok(());
            }
        }
        Err(ApiError::state(format!("the daemon didn't start; see {}", log_path.display())))
    }

    /// Opens game `game` of pair `pair` of test `id` in the session of
    /// `session`: a running game is followed, a finished one returned.
    pub async fn open_game(
        &self,
        session: SharedSession,
        controller: Controller,
        events: Events,
        id: &str,
        pair: u32,
        game: u8,
    ) -> Result<Opened, ApiError> {
        let client = self.client()?;
        // Listen first, so no move falls between the snapshot and the events.
        let stream = client.events().await.map_err(api)?;
        let live = match client.live(id, pair, game).await {
            Ok(live) => live,
            Err(ClientError::Daemon(_)) => {
                let text = client.record(id, pair, game).await.map_err(api)?;
                return Ok(Opened::Finished(GameRecord::parse(&text)?));
            }
            Err(e) => return Err(api(e)),
        };
        let info = client.info(id).await.map_err(api)?;
        let tc: Option<TimeControl> = live.time_control.as_deref().and_then(|t| t.parse().ok());
        let generation = {
            let mut s = lock(&session);
            s.start_match(
                [Player::Remote { name: live.gold.clone() }, Player::Remote { name: live.silver.clone() }],
                [tc; 2],
                false,
            );
            let mut tags = vec![
                ("Event".to_string(), info.name.clone()),
                ("Round".to_string(), format!("{pair}.{game}")),
                ("Gold".to_string(), live.gold.clone()),
                ("Silver".to_string(), live.silver.clone()),
            ];
            if let Some(tc) = &live.time_control {
                tags.push(("TimeControl".into(), tc.clone()));
            }
            s.set_tags(tags);
            let generation = s.generation();
            s.sync_remote(generation, &live.moves, live.used_ms.last().map(|ms| Duration::from_millis(*ms)))?;
            if let Some(r) = live.reserves_ms {
                let _ = s.set_remote_clock(generation, clock(r, Duration::from_millis(live.turn_elapsed_ms)));
            }
            emit_session(&events, &s, Vec::new(), None);
            generation
        };
        controller.poke();
        let follow = Follow {
            client,
            session: Arc::downgrade(&session),
            controller,
            events,
            generation,
            test: id.to_string(),
            pair,
            game,
            moves: live.moves,
        };
        tauri::async_runtime::spawn(follow.run(stream));
        Ok(Opened::Following)
    }

    /// Starts passing on the daemon's events, once.
    fn forward(self: &Arc<Self>) {
        if self.forwarding.swap(true, Ordering::SeqCst) {
            return;
        }
        let me = self.clone();
        tauri::async_runtime::spawn(async move { me.forward_loop().await });
    }

    async fn forward_loop(self: Arc<Self>) {
        let mut names: HashMap<String, [String; 2]> = HashMap::new();
        loop {
            let stream = match self.client() {
                Ok(c) => c.events().await.ok().map(|s| (c, s)),
                Err(_) => None,
            };
            if let Some((client, mut stream)) = stream {
                self.update(true, None, None, None);
                while let Some(Ok(event)) = stream.next().await {
                    match event {
                        DaemonEvent::TestChanged { test } => {
                            names.insert(test.id.clone(), test.engines.clone());
                            self.update(true, Some(test_view(&test)), None, None);
                        }
                        DaemonEvent::GameFinished { test, entry } => {
                            if !names.contains_key(&test)
                                && let Ok(info) = client.info(&test).await
                            {
                                names.insert(test.clone(), info.engines);
                            }
                            let engines = names.get(&test).cloned().unwrap_or_default();
                            self.update(true, None, Some(game_view(&test, &engines, &entry)), None);
                        }
                        DaemonEvent::SlotsChanged { slots } => self.update(true, None, None, Some(slots)),
                        _ => {}
                    }
                }
                self.update(false, None, None, None);
            }
            tokio::time::sleep(RECONNECT).await;
        }
    }

    fn update(
        &self,
        connected: bool,
        test: Option<TestView>,
        game: Option<TestGameView>,
        slots: Option<u32>,
    ) {
        emit(&self.events, TOURNEY_CHANGED, TourneyUpdate { connected, test, game, slots });
    }
}

/// A copy of the daemon's program to run, for the two installs that
/// can't run it in place, kept in the data directory's `bin/`: an
/// AppImage (its files go away when the app exits, and the daemon goes on)
/// and Windows (a running program can't be replaced, so a daemon would
/// block updating the app). The copy is refreshed when the app's is newer.
fn runnable_copy(program: &Path, data_dir: &Path) -> Result<PathBuf, ApiError> {
    let copy = data_dir.join("bin").join(DAEMON_PROGRAM);
    let modified = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    let stale = match (modified(program), modified(&copy)) {
        (Some(app), Some(ours)) => app > ours,
        (_, None) => true,
        (None, Some(_)) => false,
    };
    if stale {
        let io = |e: std::io::Error| ApiError::state(format!("couldn't copy {}: {e}", program.display()));
        std::fs::create_dir_all(copy.parent().expect("in bin/")).map_err(io)?;
        if let Err(e) = std::fs::copy(program, &copy) {
            // A daemon still running from the copy holds it; use it as it is.
            if !copy.is_file() {
                return Err(io(e));
            }
        }
    }
    Ok(copy)
}

/// `howdah-tourney` beside the app's own program (a release bundles it as
/// a sidecar; in development `cargo build -p howdah-tourney` puts it there).
fn daemon_program() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let program = exe.parent()?.join(DAEMON_PROGRAM);
    program.is_file().then_some(program)
}

fn clock(reserves_ms: [u64; 2], turn_elapsed: Duration) -> RemoteClock {
    RemoteClock {
        reserves: reserves_ms.map(Duration::from_millis),
        turn_elapsed,
        game_elapsed: None,
        running: true,
        turn_started: Instant::now().checked_sub(turn_elapsed),
        game_started: None,
    }
}

/// A running game a session follows.
struct Follow {
    client: Client,
    session: Weak<Mutex<Session>>,
    controller: Controller,
    events: Events,
    generation: u64,
    test: String,
    pair: u32,
    game: u8,
    moves: Vec<String>,
}

impl Follow {
    /// Plays the game's moves into the session as the daemon reports them,
    /// until it ends, the session moves on to another game, or its window
    /// closes.
    async fn run(mut self, mut stream: EventStream) {
        while let Some(Ok(event)) = stream.next().await {
            let ours = |t: &str, p: u32, g: u8| t == self.test && p == self.pair && g == self.game;
            match event {
                DaemonEvent::MovePlayed { test, pair, game, ply, notation, used_ms, reserves_ms }
                    if ours(&test, pair, game) =>
                {
                    if ply < self.moves.len() {
                        continue;
                    }
                    if ply == self.moves.len() {
                        self.moves.push(notation);
                    } else if let Ok(live) = self.client.live(&self.test, self.pair, self.game).await {
                        // Events were missed; catch up from the daemon.
                        self.moves = live.moves;
                    }
                    let Some(session) = self.session.upgrade() else { return };
                    let mut s = lock(&session);
                    if s.generation() != self.generation {
                        return;
                    }
                    let Ok(animation) =
                        s.sync_remote(self.generation, &self.moves, Some(Duration::from_millis(used_ms)))
                    else {
                        return;
                    };
                    if let Some(r) = reserves_ms {
                        let _ = s.set_remote_clock(self.generation, clock(r, Duration::ZERO));
                    }
                    emit_session(&self.events, &s, animation, None);
                    drop(s);
                    self.controller.poke();
                }
                DaemonEvent::GameFinished { test, entry } if ours(&test, entry.pair, entry.game) => {
                    let Some(session) = self.session.upgrade() else { return };
                    let mut s = lock(&session);
                    if let Some(result) = entry.result()
                        && s.finish_remote(self.generation, result, entry.detail.clone()).is_ok()
                    {
                        emit_session(&self.events, &s, Vec::new(), None);
                    }
                    return;
                }
                _ => {}
            }
        }
    }
}

/// Signed with one decimal, or ±inf.
fn signed(x: f64) -> String {
    if x == f64::INFINITY {
        "+inf".into()
    } else if x == f64::NEG_INFINITY {
        "-inf".into()
    } else {
        format!("{:+.1}", x + 0.0)
    }
}

pub fn test_view(t: &TestInfo) -> TestView {
    let s = &t.summary;
    let (outcome, halted) = match &t.end {
        Some(RunEnd::Decided(Decision::Pass)) => (Some("passed"), None),
        Some(RunEnd::Decided(Decision::Fail)) => (Some("failed"), None),
        Some(RunEnd::MaxPairs) => (Some("done"), None),
        Some(RunEnd::Stopped) => (Some("stopped"), None),
        Some(RunEnd::Halted(why)) => (Some("halted"), Some(why.clone())),
        None => (None, None),
    };
    let elo = s.elo.map(|e| match s.elo_interval {
        Some((lo, hi)) => format!("{} [{}, {}]", signed(e), signed(lo), signed(hi)),
        None => signed(e),
    });
    TestView {
        id: t.id.clone(),
        name: t.name.clone(),
        engines: t.engines.clone(),
        time_control: t.time_control.clone(),
        status: match t.status {
            TestStatus::Active => "active",
            TestStatus::Paused => "paused",
            TestStatus::Finished => "finished",
        }
        .into(),
        outcome: outcome.map(str::to_string),
        halted,
        priority: t.priority,
        max_pairs: t.max_pairs,
        concurrency: t.concurrency,
        sprt: t.sprt.map(|s| SprtView { elo0: s.elo0, elo1: s.elo1 }),
        running: t.running.iter().map(|&(pair, game)| TestGameRef { pair, game }).collect(),
        games: s.games,
        pairs: s.pairs,
        wins: s.wins,
        pair_counts: s.pair_counts,
        score: s.score,
        elo,
        los: s.los,
        llr: s.llr,
        llr_bounds: s.llr_bounds,
        repeated: s.repeated,
        path: t.path.display().to_string(),
    }
}

fn game_view(test: &str, engines: &[String], g: &GameEntry) -> TestGameView {
    let name = |i: usize| engines.get(i).cloned().unwrap_or_else(|| format!("engine {}", i + 1));
    let gold = g.gold();
    TestGameView {
        test: test.to_string(),
        pair: g.pair,
        game: g.game,
        gold: name(gold),
        silver: name(1 - gold),
        winner: if g.winner == gold { Color::Gold } else { Color::Silver },
        reason: g.reason.clone(),
        plies: g.plies,
        detail: g.detail.clone(),
    }
}

/// The daemon's spec for a test of two engines from the engine list.
pub fn test_spec(new: NewTestSpec, engines: [EngineSpec; 2]) -> Result<TestSpec, ApiError> {
    let names = {
        let [a, b] = [&engines[0].name, &engines[1].name];
        // Testing an engine against itself still needs two names.
        if a == b { [format!("{a} (1)"), format!("{b} (2)")] } else { [a.clone(), b.clone()] }
    };
    let mut specs = Vec::new();
    for (e, name) in engines.into_iter().zip(names) {
        let program = std::path::absolute(&e.program)
            .map_err(|err| ApiError::state(format!("{}: {err}", e.program)))?;
        let options: BTreeMap<String, OptionValue> =
            e.options.into_iter().map(|o| (o.name, OptionValue::Text(o.value))).collect();
        specs.push(howdah_tourney::EngineSpec {
            name,
            program,
            args: e.args,
            working_dir: e.working_dir.filter(|d| !d.trim().is_empty()).map(PathBuf::from),
            options,
            restart_each_game: false,
            // An installed engine may need the files beside it.
            copy_dir: e.installed.is_some(),
            source: None,
            sha256: None,
        });
    }
    let spec = TestSpec {
        name: new.name.trim().to_string(),
        time_control: new.time_control.filter(|t| !t.trim().is_empty()).map(|t| t.trim().to_string()),
        concurrency: new.concurrency,
        max_pairs: new.max_pairs,
        sprt: new.sprt.map(|s| SprtSpec { elo0: s.elo0, elo1: s.elo1, alpha: 0.05, beta: 0.05 }),
        engine_logs: false,
        engines: specs,
    };
    if spec.name.is_empty() {
        return Err(ApiError::state("the test needs a name"));
    }
    spec.validate().map_err(ApiError::state)?;
    Ok(spec)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::{EngineOption, InstalledFrom};

    fn engine(name: &str, installed: bool) -> EngineSpec {
        EngineSpec {
            id: name.to_lowercase(),
            name: name.into(),
            program: format!("/bots/{name}"),
            args: vec!["aei".into()],
            working_dir: Some(String::new()),
            options: vec![EngineOption { name: "hash".into(), value: "64".into() }],
            installed: installed.then(|| InstalledFrom { manifest: "m".into(), version: "1".into() }),
        }
    }

    fn new_test(sprt: bool, max_pairs: Option<u32>) -> NewTestSpec {
        NewTestSpec {
            name: " dev ".into(),
            engines: ["a".into(), "b".into()],
            time_control: Some(" 2s/30s/100/0/80t ".into()),
            max_pairs,
            sprt: sprt.then_some(SprtView { elo0: 0.0, elo1: 10.0 }),
            concurrency: 2,
            priority: 0,
        }
    }

    #[test]
    fn a_test_spec_from_the_engine_list() {
        let spec = test_spec(new_test(true, None), [engine("Sharp", true), engine("OpFor", false)]).unwrap();
        assert_eq!(spec.name, "dev");
        assert_eq!(spec.time_control.as_deref(), Some("2s/30s/100/0/80t"));
        let [a, b] = [&spec.engines[0], &spec.engines[1]];
        assert_eq!((a.name.as_str(), b.name.as_str()), ("Sharp", "OpFor"));
        assert!(a.copy_dir && !b.copy_dir, "installed engines bring their directory");
        assert_eq!(a.working_dir, None);
        assert_eq!(a.options["hash"].to_string(), "64");
        assert_eq!(spec.sprt.unwrap().alpha, 0.05);
    }

    #[test]
    fn an_engine_against_itself_gets_two_names() {
        let spec =
            test_spec(new_test(false, Some(10)), [engine("Sharp", false), engine("Sharp", false)]).unwrap();
        assert_eq!(spec.engines[0].name, "Sharp (1)");
        assert_eq!(spec.engines[1].name, "Sharp (2)");
    }

    #[test]
    fn the_daemon_runs_from_a_copy_refreshed_when_the_app_is_newer() {
        let tmp = tempfile::tempdir().unwrap();
        let program = tmp.path().join("app").join(DAEMON_PROGRAM);
        std::fs::create_dir_all(program.parent().unwrap()).unwrap();
        std::fs::write(&program, b"v1").unwrap();
        let data = tmp.path().join("data");
        let copy = runnable_copy(&program, &data).unwrap();
        assert_eq!(copy, data.join("bin").join(DAEMON_PROGRAM));
        assert_eq!(std::fs::read(&copy).unwrap(), b"v1");

        // An older app program leaves the copy alone; a newer one replaces it.
        std::fs::write(&copy, b"kept").unwrap();
        runnable_copy(&program, &data).unwrap();
        assert_eq!(std::fs::read(&copy).unwrap(), b"kept");
        let later = std::time::SystemTime::now() + Duration::from_secs(60);
        std::fs::write(&program, b"v2").unwrap();
        std::fs::File::options().write(true).open(&program).unwrap().set_modified(later).unwrap();
        runnable_copy(&program, &data).unwrap();
        assert_eq!(std::fs::read(&copy).unwrap(), b"v2");
    }

    #[test]
    fn a_test_needs_a_way_to_end() {
        let err = test_spec(new_test(false, None), [engine("A", false), engine("B", false)]).unwrap_err();
        assert!(err.message.contains("so the test ends"), "{}", err.message);
    }
}
