//! Engine tests: the app as a client of howdah-tourney's daemon (plan in
//! `docs/TOURNAMENTS.md`). The lobby lists the daemon's tests and changes
//! them; the daemon's events reach it as `tourney://changed` while the app
//! is connected (it looks for a daemon every few seconds). A test's game
//! opens in a game window: a finished one as its record, a running one
//! followed like a watched arimaa.com game, between two remote players
//! whose moves and clocks come from the daemon, as does the engines'
//! search output for the engine panels.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use howdah_aei::{EngineId, EngineMessage, Profile};
use howdah_arimaa::{Color, DEFAULT_SETUP_MINIMUM, GameRecord, SETUP_MINIMUM_TAG, TimeControl};
use howdah_tourney::client::{Client, ClientError, EventStream};
use howdah_tourney::{
    DaemonEvent, DaemonInfo, Decision, EngineLine, GameEntry, GameOutput, OptionValue, PoolInfo, PoolSpec,
    RunEnd, SprtSpec, TestInfo, TestSpec, TestStatus,
};

use crate::backend::{Events, emit, emit_session};
use crate::controller::{Controller, ENGINE_OUTPUT, SharedSession, engine_output};
use crate::dto::{
    ApiError, ArchivedTestView, EngineOutput, EngineSpec, MatchupView, NewTestSpec, PoolEngineView, PoolView,
    ProjectionView, SprtView, SpsaParamView, SpsaStepView, SpsaView, TestDetailView, TestGameRef,
    TestGameTag, TestGameView, TestView, TourneyStatusView, TourneyUpdate,
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
    /// It's finished; this is its record, and the test's name.
    Finished(GameRecord, String),
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
            closing: status.as_ref().is_some_and(|s| s.closing),
        }
    }

    pub async fn tests(&self) -> Result<Vec<TestView>, ApiError> {
        Ok(self.client()?.list().await.map_err(api)?.iter().map(test_view).collect())
    }

    pub async fn test(&self, id: &str) -> Result<TestDetailView, ApiError> {
        let client = self.client()?;
        let info = client.info(id).await.map_err(api)?;
        let names = all_names(&info);
        let mut games: Vec<TestGameView> =
            client.games(id).await.map_err(api)?.iter().map(|g| game_view(id, &names, g)).collect();
        games.reverse();
        Ok(TestDetailView { test: test_view(&info), games })
    }

    /// An SPSA run's steps after the first `skip`, θ in the order of its
    /// parameters.
    pub async fn spsa_history(&self, id: &str, skip: usize) -> Result<Vec<SpsaStepView>, ApiError> {
        let client = self.client()?;
        let info = client.info(id).await.map_err(api)?;
        let names: Vec<String> =
            info.spsa.map(|s| s.params.into_iter().map(|p| p.name).collect()).unwrap_or_default();
        let steps = client.spsa_history(id, skip).await.map_err(api)?;
        Ok(steps
            .into_iter()
            .map(|s| SpsaStepView {
                iteration: s.iteration,
                wins: s.wins,
                losses: s.losses,
                theta: names.iter().map(|n| s.theta.get(n).copied().unwrap_or(f64::NAN)).collect(),
                finished: s.finished,
            })
            .collect())
    }

    /// Adds a test of two engines from the engine list, or a pool.
    pub async fn submit(&self, new: NewTestSpec, engines: Vec<EngineSpec>) -> Result<TestView, ApiError> {
        let spec = test_spec(new.clone(), engines)?;
        Ok(test_view(&self.client()?.submit(&spec, new.priority).await.map_err(api)?))
    }

    /// Adds an engine from the engine list to a pool, named apart from
    /// its engines (`Sharp (2)`) if it has to be.
    pub async fn add_engine(&self, id: &str, engine: EngineSpec) -> Result<TestView, ApiError> {
        let client = self.client()?;
        let info = client.info(id).await.map_err(api)?;
        let taken = all_names(&info);
        let name = (1..)
            .map(|n| if n == 1 { engine.name.clone() } else { format!("{} ({n})", engine.name) })
            .find(|n| !taken.contains(n))
            .expect("some name is free");
        let engine = daemon_engine(engine, name)?;
        Ok(test_view(&client.add_engine(id, &engine).await.map_err(api)?))
    }

    /// Retires a pool's engine (it plays no new pairs), or brings it back.
    pub async fn set_engine_retired(
        &self,
        id: &str,
        name: &str,
        retired: bool,
    ) -> Result<TestView, ApiError> {
        Ok(test_view(&self.client()?.set_engine_retired(id, name, retired).await.map_err(api)?))
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

    pub async fn set_concurrency(&self, id: &str, concurrency: u32) -> Result<TestView, ApiError> {
        Ok(test_view(&self.client()?.set_concurrency(id, concurrency).await.map_err(api)?))
    }

    pub async fn set_max_pairs(&self, id: &str, max_pairs: Option<u32>) -> Result<TestView, ApiError> {
        Ok(test_view(&self.client()?.set_max_pairs(id, max_pairs).await.map_err(api)?))
    }

    pub async fn set_undecided_below(&self, id: &str, cutoff: Option<f64>) -> Result<TestView, ApiError> {
        Ok(test_view(&self.client()?.set_undecided_below(id, cutoff).await.map_err(api)?))
    }

    /// Takes a test that isn't active out of the daemon's queue: zipped
    /// into its `archive/` (whose path is returned) if `archive`, else
    /// deleted.
    pub async fn remove(&self, id: &str, archive: bool) -> Result<Option<String>, ApiError> {
        let client = self.client()?;
        if archive {
            Ok(Some(client.archive(id).await.map_err(api)?.display().to_string()))
        } else {
            client.delete(id).await.map_err(api).map(|()| None)
        }
    }

    /// The archived tests, newest first.
    pub async fn archived(&self) -> Result<Vec<ArchivedTestView>, ApiError> {
        let list = self.client()?.archived().await.map_err(api)?;
        Ok(list
            .into_iter()
            .map(|a| ArchivedTestView {
                file: a.file,
                size: a.size,
                archived: a.archived,
                test: a.test.as_ref().map(test_view),
                error: a.error,
            })
            .collect())
    }

    /// Puts archived test `file` back in the queue.
    pub async fn restore(&self, file: &str) -> Result<TestView, ApiError> {
        Ok(test_view(&self.client()?.restore(file).await.map_err(api)?))
    }

    pub async fn delete_archived(&self, file: &str) -> Result<(), ApiError> {
        self.client()?.delete_archived(file).await.map_err(api)
    }

    pub async fn set_slots(&self, slots: u32) -> Result<(), ApiError> {
        self.client()?.set_slots(slots).await.map_err(api).map(drop)
    }

    pub async fn shutdown(&self, now: bool) -> Result<(), ApiError> {
        self.client()?.shutdown(now).await.map_err(api).map(drop)
    }

    /// Calls off a shutdown that's waiting for the running games.
    pub async fn cancel_shutdown(&self) -> Result<(), ApiError> {
        self.client()?.cancel_shutdown().await.map_err(api).map(drop)
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
        let mut child = spawn_detached(&mut cmd)
            .map_err(|e| ApiError::state(format!("couldn't start {}: {e}", program.display())))?;
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
                let info = client.info(id).await.map_err(api)?;
                return Ok(Opened::Finished(GameRecord::parse(&text)?, info.name));
            }
            Err(e) => return Err(api(e)),
        };
        let info = client.info(id).await.map_err(api)?;
        // From here on; what came before is in the snapshot. A daemon
        // without the stream only gives moves.
        let output = client.output(id, pair, game).await.ok();
        let profiles =
            live.engine_ids.clone().map(|name| Profile::detect(&EngineId { name, ..EngineId::default() }));
        let setup_minimum = live
            .setup_minimum
            .as_deref()
            .and_then(|t| TimeControl::parse_time(t).ok())
            .unwrap_or(DEFAULT_SETUP_MINIMUM);
        let tc: Option<TimeControl> = live
            .time_control
            .as_deref()
            .and_then(|t| t.parse().ok())
            .map(|t: TimeControl| t.with_setup_minimum(setup_minimum));
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
            if let Some(minimum) = &live.setup_minimum {
                tags.push((SETUP_MINIMUM_TAG.into(), minimum.clone()));
            }
            s.set_tags(tags);
            s.set_test_game(TestGameTag { test: id.to_string(), name: info.name.clone(), pair, game });
            let generation = s.generation();
            s.sync_remote(generation, &live.moves, live.used_ms.last().map(|ms| Duration::from_millis(*ms)))?;
            // The moves so far arrive together, without times; the
            // daemon has them all.
            let used: Vec<Duration> = live.used_ms.iter().map(|&ms| Duration::from_millis(ms)).collect();
            s.fill_move_times(generation, &used);
            for (i, side) in Color::ALL.into_iter().enumerate() {
                for line in &live.output[i] {
                    if let Some(out) = panel_output(side, profiles[i], line.clone()) {
                        s.record_engine_output(out);
                    }
                }
            }
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
            profiles,
        };
        tauri::async_runtime::spawn(follow.run(stream, output));
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
        let mut names: HashMap<String, Vec<String>> = HashMap::new();
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
                            names.insert(test.id.clone(), all_names(&test));
                            self.update(true, Some(test_view(&test)), None, None);
                        }
                        // The daemon sends no TestChanged as a game starts,
                        // so its test's running games come from asking.
                        DaemonEvent::GameStarted { test, .. } => {
                            if let Ok(info) = client.info(&test).await {
                                names.insert(test, all_names(&info));
                                self.update(true, Some(test_view(&info)), None, None);
                            }
                        }
                        DaemonEvent::GameFinished { test, entry } => {
                            if !names.contains_key(&test)
                                && let Ok(info) = client.info(&test).await
                            {
                                names.insert(test.clone(), all_names(&info));
                            }
                            let engines = names.get(&test).cloned().unwrap_or_default();
                            self.update(true, None, Some(game_view(&test, &engines, &entry)), None);
                        }
                        DaemonEvent::TestRemoved { test, .. } => {
                            names.remove(&test);
                            let update = TourneyUpdate {
                                connected: true,
                                test: None,
                                game: None,
                                slots: None,
                                removed: Some(test),
                                closing: None,
                            };
                            emit(&self.events, TOURNEY_CHANGED, update);
                        }
                        DaemonEvent::SlotsChanged { slots } => self.update(true, None, None, Some(slots)),
                        DaemonEvent::ShuttingDown { .. } => self.closing(true),
                        DaemonEvent::ShutdownCancelled => self.closing(false),
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
        emit(
            &self.events,
            TOURNEY_CHANGED,
            TourneyUpdate { connected, test, game, slots, removed: None, closing: None },
        );
    }

    fn closing(&self, closing: bool) {
        let update = TourneyUpdate {
            connected: true,
            test: None,
            game: None,
            slots: None,
            removed: None,
            closing: Some(closing),
        };
        emit(&self.events, TOURNEY_CHANGED, update);
    }
}

/// Starts the daemon out of the app's process group (and console), so it
/// outlives the app and a Ctrl-C meant for it. On Windows it also breaks
/// away from the app's job: `cargo run` (so `tauri dev`) puts the app in
/// a job that's killed with cargo, which took a running test's daemon down
/// on every rebuild. Cargo's job allows breaking away; a job that doesn't
/// refuses the flag, and the daemon then starts inside it.
fn spawn_detached(cmd: &mut std::process::Command) -> std::io::Result<std::process::Child> {
    #[cfg(unix)]
    {
        std::os::unix::process::CommandExt::process_group(cmd, 0);
        cmd.spawn()
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
        const ERROR_ACCESS_DENIED: i32 = 5;
        let flags = CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW;
        match cmd.creation_flags(flags | CREATE_BREAKAWAY_FROM_JOB).spawn() {
            Err(e) if e.raw_os_error() == Some(ERROR_ACCESS_DENIED) => cmd.creation_flags(flags).spawn(),
            spawned => spawned,
        }
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
    /// How to read each side's output (gold, silver).
    profiles: [Profile; 2],
}

/// An engine line from the daemon as the engine panels show it.
fn panel_output(side: Color, profile: Profile, line: EngineLine) -> Option<EngineOutput> {
    let msg = match line {
        EngineLine::Info(info) => EngineMessage::Info(info),
        EngineLine::Log(text) => EngineMessage::Log(text),
    };
    engine_output(side, profile, msg)
}

/// The next line from a game's output stream; never, once it's gone.
async fn next_output(output: &mut Option<EventStream<GameOutput>>) -> GameOutput {
    if let Some(stream) = output
        && let Some(Ok(o)) = stream.next().await
    {
        return o;
    }
    *output = None;
    std::future::pending().await
}

impl Follow {
    /// Plays the game's moves into the session as the daemon reports them,
    /// until it ends, the session moves on to another game, or its window
    /// closes.
    async fn run(mut self, mut stream: EventStream, mut output: Option<EventStream<GameOutput>>) {
        loop {
            let event = tokio::select! {
                e = stream.next() => match e {
                    Some(Ok(e)) => e,
                    _ => return,
                },
                o = next_output(&mut output) => {
                    if !self.engine_line(o) {
                        return;
                    }
                    continue;
                }
            };
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

    /// Passes on an engine's output in the game to the session's panels;
    /// false once the session has moved on or closed.
    fn engine_line(&self, o: GameOutput) -> bool {
        if (o.test.as_str(), o.pair, o.game) != (self.test.as_str(), self.pair, self.game) {
            return true;
        }
        let Some(session) = self.session.upgrade() else { return false };
        let Some(out) = panel_output(o.side, self.profiles[o.side.index()], o.line) else { return true };
        let out = {
            let mut s = lock(&session);
            if s.generation() != self.generation {
                return false;
            }
            s.record_engine_output(out)
        };
        emit(&self.events, ENGINE_OUTPUT, out);
        true
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

/// Every engine's name, in the daemon's order: a pool's, or a test's two.
fn all_names(t: &TestInfo) -> Vec<String> {
    match &t.pool {
        Some(p) => p.engines.iter().map(|e| e.name.clone()).collect(),
        None => t.engines.to_vec(),
    }
}

fn pool_view(p: &PoolInfo) -> PoolView {
    PoolView {
        engines: p
            .engines
            .iter()
            .map(|e| PoolEngineView {
                name: e.name.clone(),
                retired: e.retired,
                games: e.games,
                wins: e.wins,
                elo: e.elo,
                error: e.error,
            })
            .collect(),
        ranking: p.ranking.iter().map(|&i| i as u32).collect(),
        anchor: p.anchor.map(|a| a as u32),
        matchups: p
            .matchups
            .iter()
            .map(|m| MatchupView {
                a: m.a as u32,
                b: m.b as u32,
                wins: m.wins,
                pairs: m.pairs,
                expected: m.expected,
                z: m.z,
                outlier: m.outlier,
            })
            .collect(),
        min_pairs: p.min_pairs,
        outlier_z: p.outlier_z,
    }
}

pub fn test_view(t: &TestInfo) -> TestView {
    let s = &t.summary;
    let (outcome, halted) = match &t.end {
        Some(RunEnd::Decided(Decision::Pass)) => (Some("passed"), None),
        Some(RunEnd::Decided(Decision::Fail)) => (Some("failed"), None),
        Some(RunEnd::MaxPairs) => (Some("done"), None),
        Some(RunEnd::Undecided) => (Some("undecided"), None),
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
        sprt: t.sprt.map(|s| SprtView { elo0: s.elo0, elo1: s.elo1, undecided_below: t.undecided_below }),
        running: t
            .running
            .iter()
            .enumerate()
            .map(|(i, &(pair, game))| {
                // A pool's pairs each have their own engines.
                let engines = t.running_engines.get(i).unwrap_or(&t.engines);
                let gold = GameEntry::gold_of(game);
                TestGameRef { pair, game, gold: engines[gold].clone(), silver: engines[1 - gold].clone() }
            })
            .collect(),
        games: s.games,
        pairs: s.pairs,
        wins: s.wins,
        pair_counts: s.pair_counts,
        score: s.score,
        elo,
        los: s.los,
        llr: s.llr,
        llr_bounds: s.llr_bounds,
        decide_chance: t.decide_chance,
        projection: t.projection.map(|p| ProjectionView {
            pass: p.pass,
            fail: p.fail,
            undecided: p.undecided,
            done: p.done,
            games_left: p.games_left,
            games_if_decided: p.games_if_decided,
            games_if_not: p.games_if_not,
            open_ended: t.max_pairs.is_none() && p.pairs_left[2] >= howdah_tourney::NO_LIMIT_HORIZON,
        }),
        repeated: s.repeated,
        spsa: t.spsa.as_ref().map(|s| SpsaView {
            iterations: s.iterations,
            pairs_per_iteration: s.pairs_per_iteration,
            done: s.done,
            params: s
                .params
                .iter()
                .map(|p| SpsaParamView {
                    name: p.name.clone(),
                    value: p.value,
                    start: p.start,
                    min: p.min,
                    max: p.max,
                    int: p.int,
                    c: p.c,
                })
                .collect(),
        }),
        pool: t.pool.as_ref().map(pool_view),
        path: t.path.display().to_string(),
    }
}

/// A finished game, its engines named from every engine's name (a pool's
/// pairs each have their own).
fn game_view(test: &str, all: &[String], g: &GameEntry) -> TestGameView {
    let names = g.names(all);
    let gold = g.gold();
    TestGameView {
        test: test.to_string(),
        pair: g.pair,
        game: g.game,
        gold: names[gold].clone(),
        silver: names[1 - gold].clone(),
        winner: if g.winner == gold { Color::Gold } else { Color::Silver },
        reason: g.reason.clone(),
        plies: g.plies,
        detail: g.detail.clone(),
        finished: g.finished,
    }
}

/// An engine-list entry as the daemon takes it, named `name`.
fn daemon_engine(e: EngineSpec, name: String) -> Result<howdah_tourney::EngineSpec, ApiError> {
    let program =
        std::path::absolute(&e.program).map_err(|err| ApiError::state(format!("{}: {err}", e.program)))?;
    let options: BTreeMap<String, OptionValue> =
        e.options.into_iter().map(|o| (o.name, OptionValue::Text(o.value))).collect();
    Ok(howdah_tourney::EngineSpec {
        name,
        program,
        args: e.args,
        working_dir: e.working_dir.filter(|d| !d.trim().is_empty()).map(PathBuf::from),
        options,
        restart_each_game: false,
        // An installed engine may need the files beside it.
        copy_dir: e.installed.is_some(),
        retired: false,
        source: None,
        sha256: None,
    })
}

/// The daemon's spec for a test of two engines from the engine list, or
/// a pool of them.
pub fn test_spec(new: NewTestSpec, engines: Vec<EngineSpec>) -> Result<TestSpec, ApiError> {
    if new.pool && engines.len() < 2 {
        return Err(ApiError::state("a pool needs at least two engines"));
    }
    if !new.pool && engines.len() != 2 {
        return Err(ApiError::state("a test needs two engines"));
    }
    // Testing an engine against itself still needs two names.
    let count = |name: &str| engines.iter().filter(|e| e.name == name).count();
    let mut seen: HashMap<String, usize> = HashMap::new();
    let names: Vec<String> = engines
        .iter()
        .map(|e| {
            let n = seen.entry(e.name.clone()).or_insert(0);
            *n += 1;
            if count(&e.name) > 1 { format!("{} ({n})", e.name) } else { e.name.clone() }
        })
        .collect();
    let specs =
        engines.into_iter().zip(names).map(|(e, name)| daemon_engine(e, name)).collect::<Result<_, _>>()?;
    let spec = TestSpec {
        name: new.name.trim().to_string(),
        time_control: new.time_control.filter(|t| !t.trim().is_empty()).map(|t| t.trim().to_string()),
        // Engines get no setup minute.
        setup_minimum: None,
        concurrency: new.concurrency,
        max_pairs: new.max_pairs,
        sprt: new.sprt.filter(|_| !new.pool).map(|s| SprtSpec {
            elo0: s.elo0,
            elo1: s.elo1,
            alpha: 0.05,
            beta: 0.05,
            undecided_below: s.undecided_below,
        }),
        spsa: None,
        pool: new.pool.then(PoolSpec::default),
        engine_logs: false,
        engines: specs,
        memory: Default::default(),
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
            engines: vec!["a".into(), "b".into()],
            pool: false,
            time_control: Some(" 2s/30s/100/0/80t ".into()),
            max_pairs,
            sprt: sprt.then_some(SprtView { elo0: 0.0, elo1: 10.0, undecided_below: Some(0.05) }),
            concurrency: 2,
            priority: 0,
        }
    }

    #[test]
    fn a_test_spec_from_the_engine_list() {
        let spec =
            test_spec(new_test(true, None), vec![engine("Sharp", true), engine("OpFor", false)]).unwrap();
        assert_eq!(spec.name, "dev");
        assert_eq!(spec.time_control.as_deref(), Some("2s/30s/100/0/80t"));
        let [a, b] = [&spec.engines[0], &spec.engines[1]];
        assert_eq!((a.name.as_str(), b.name.as_str()), ("Sharp", "OpFor"));
        assert!(a.copy_dir && !b.copy_dir, "installed engines bring their directory");
        assert_eq!(a.working_dir, None);
        assert_eq!(a.options["hash"].to_string(), "64");
        assert_eq!(spec.sprt.unwrap().alpha, 0.05);
        assert_eq!(spec.sprt.unwrap().undecided_below, Some(0.05));
    }

    #[test]
    fn an_engine_against_itself_gets_two_names() {
        let spec = test_spec(new_test(false, Some(10)), vec![engine("Sharp", false), engine("Sharp", false)])
            .unwrap();
        assert_eq!(spec.engines[0].name, "Sharp (1)");
        assert_eq!(spec.engines[1].name, "Sharp (2)");
    }

    #[test]
    fn a_pool_from_the_engine_list() {
        let new = NewTestSpec { pool: true, ..new_test(true, None) };
        let engines = vec![engine("Sharp", true), engine("OpFor", false), engine("Sharp", false)];
        let spec = test_spec(new.clone(), engines).unwrap();
        assert!(spec.pool.is_some() && spec.sprt.is_none(), "a pool has no SPRT");
        let names: Vec<&str> = spec.engines.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["Sharp (1)", "OpFor", "Sharp (2)"]);
        assert!(test_spec(new, vec![engine("A", false)]).unwrap_err().message.contains("at least two"));
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
        let err = test_spec(new_test(false, None), vec![engine("A", false), engine("B", false)]).unwrap_err();
        assert!(err.message.contains("so the test ends"), "{}", err.message);
    }
}
