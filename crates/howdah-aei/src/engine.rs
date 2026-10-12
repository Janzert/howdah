//! A running AEI engine process.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use howdah_arimaa::{Position, TimeControl};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::time::{Instant, timeout_at};

use crate::error::AeiError;
use crate::message::EngineMessage;
use crate::process;

/// How to launch an engine. The program is run directly (no shell).
#[derive(Clone, Debug)]
pub struct EngineConfig {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub working_dir: Option<PathBuf>,
    /// How long to wait for `aeiok` after sending `aei`.
    pub handshake_timeout: Duration,
    /// Starts the engine in a process group of its own, so a Ctrl-C in our
    /// terminal (which goes to the whole foreground group) doesn't reach
    /// it and the controller decides when it stops. Our own death still
    /// ends it as before (`kill_on_drop`, and on Linux `kill_with_parent`).
    pub own_process_group: bool,
    /// A memory cap in bytes for the kernel to enforce, where it can
    /// ([`process::memory_cap_method`]): an engine passing it is killed
    /// (and so fails), rather than the machine running out of memory.
    pub memory_limit: Option<u64>,
}

impl EngineConfig {
    pub fn new(program: impl Into<PathBuf>) -> EngineConfig {
        EngineConfig {
            program: program.into(),
            args: Vec::new(),
            working_dir: None,
            handshake_timeout: Duration::from_secs(10),
            own_process_group: false,
            memory_limit: None,
        }
    }

    pub fn args<I, S>(mut self, args: I) -> EngineConfig
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args = args.into_iter().map(Into::into).collect();
        self
    }

    pub fn working_dir(mut self, dir: impl Into<PathBuf>) -> EngineConfig {
        self.working_dir = Some(dir.into());
        self
    }

    pub fn own_process_group(mut self) -> EngineConfig {
        self.own_process_group = true;
        self
    }

    pub fn memory_limit(mut self, bytes: u64) -> EngineConfig {
        self.memory_limit = Some(bytes);
        self
    }
}

/// What the engine said about itself during the handshake.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EngineId {
    /// 0 if the engine didn't send `protocol-version` (pre-2009 engines).
    pub protocol_version: u32,
    pub name: Option<String>,
    pub author: Option<String>,
    pub version: Option<String>,
}

/// Direction of a line in the engine transcript.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    ToEngine,
    FromEngine,
}

type Transcript = Box<dyn FnMut(Direction, &str) + Send>;

/// A running engine that has completed the AEI handshake. The process is
/// killed when this is dropped; call [`Engine::quit`] for a clean exit.
/// Whatever it started goes with it: on Unix when it has a process group
/// of its own, on Windows always (its job object).
pub struct Engine {
    child: Child,
    stdin: ChildStdin,
    lines: Lines<BufReader<ChildStdout>>,
    id: EngineId,
    transcript: Option<Transcript>,
    /// Whether the process leads a group of its own, killed with it.
    #[cfg_attr(not(unix), allow(dead_code))]
    group: bool,
    /// Whether a memory cap applies to it.
    capped: bool,
    #[cfg(windows)]
    job: Option<process::Job>,
}

/// Has Linux kill the engine when we die. `kill_on_drop` needs our
/// destructors to run, which they don't when we're killed or exit through
/// `process::exit` (as Tauri does); the engine then only sees its stdin
/// close, and an engine that ignores that runs on as an orphan.
///
/// The signal comes when the *thread* that spawned the engine exits, not the
/// process. Engines are spawned from tokio's worker threads, which live as
/// long as the runtime; don't start one from a short-lived thread.
#[cfg(target_os = "linux")]
fn kill_with_parent(cmd: &mut Command) {
    let parent = std::process::id() as libc::pid_t;
    // SAFETY: the closure runs in the forked child before exec and calls
    // only async-signal-safe functions, without allocating.
    unsafe {
        cmd.pre_exec(move || {
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            // We may have died before the prctl.
            if libc::getppid() != parent {
                return Err(std::io::Error::from_raw_os_error(libc::ESRCH));
            }
            Ok(())
        });
    }
}

impl Engine {
    /// Starts the engine and performs the `aei` ... `aeiok` handshake.
    pub async fn start(config: &EngineConfig) -> Result<Engine, AeiError> {
        Engine::start_with_transcript(config, None).await
    }

    /// Like [`Engine::start`], calling `transcript` for every line sent or received.
    pub async fn start_with_transcript(
        config: &EngineConfig,
        transcript: Option<Transcript>,
    ) -> Result<Engine, AeiError> {
        let wrapped =
            config.memory_limit.and_then(|l| process::capped_command(&config.program, &config.args, l));
        let capped = wrapped.is_some() || (cfg!(windows) && config.memory_limit.is_some());
        let (program, args) = wrapped.unwrap_or_else(|| (config.program.clone(), config.args.clone()));
        let mut cmd = Command::new(&program);
        cmd.args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true);
        #[cfg(target_os = "linux")]
        kill_with_parent(&mut cmd);
        if config.own_process_group {
            #[cfg(unix)]
            cmd.process_group(0);
            // CREATE_NEW_PROCESS_GROUP, which also turns Ctrl-C off in it.
            #[cfg(windows)]
            cmd.creation_flags(0x0000_0200);
        }
        if let Some(dir) = &config.working_dir {
            // A missing directory makes spawn fail with the same "not found"
            // error as a missing program, so check it separately.
            if !dir.is_dir() {
                return Err(AeiError::WorkingDir(dir.display().to_string()));
            }
            cmd.current_dir(dir);
        }
        let mut child = cmd
            .spawn()
            .map_err(|source| AeiError::Spawn { program: config.program.display().to_string(), source })?;
        // Put in its job before the handshake, so it has had little time
        // to start anything outside it.
        #[cfg(windows)]
        let job = child.raw_handle().and_then(|h| process::Job::assign(h, config.memory_limit));
        let stdin = child.stdin.take().expect("stdin is piped");
        let stdout = child.stdout.take().expect("stdout is piped");
        let mut engine = Engine {
            child,
            stdin,
            lines: BufReader::new(stdout).lines(),
            id: EngineId::default(),
            transcript,
            group: config.own_process_group,
            capped,
            #[cfg(windows)]
            job,
        };
        engine.handshake(config.handshake_timeout).await?;
        Ok(engine)
    }

    async fn handshake(&mut self, timeout: Duration) -> Result<(), AeiError> {
        self.send("aei").await?;
        let deadline = Instant::now() + timeout;
        loop {
            match self.recv_until(deadline).await? {
                None => return Err(AeiError::Timeout("aeiok")),
                Some(EngineMessage::AeiOk) => return Ok(()),
                Some(EngineMessage::ProtocolVersion(v)) => {
                    self.id.protocol_version = v.trim().parse().unwrap_or(0);
                }
                Some(EngineMessage::Id { key, value }) => match key.as_str() {
                    "name" => self.id.name = Some(value),
                    "author" => self.id.author = Some(value),
                    "version" => self.id.version = Some(value),
                    _ => {}
                },
                Some(EngineMessage::Log(_)) => {}
                Some(other) => {
                    return Err(AeiError::Unexpected { expected: "aeiok", got: format!("{other:?}") });
                }
            }
        }
    }

    pub fn id(&self) -> &EngineId {
        &self.id
    }

    /// The engine's process id, while it runs.
    pub fn pid(&self) -> Option<u32> {
        self.child.id()
    }

    /// Bytes the engine's process has resident, if that can be read
    /// ([`process::resident_memory`]).
    pub fn resident_memory(&self) -> Option<u64> {
        process::resident_memory(self.pid()?)
    }

    /// Whether the kernel enforces [`EngineConfig::memory_limit`] for it.
    pub fn memory_capped(&self) -> bool {
        self.capped
    }

    /// The engine's name, or a placeholder.
    pub fn name(&self) -> &str {
        self.id.name.as_deref().unwrap_or("engine")
    }

    /// Sends one raw protocol line.
    pub async fn send(&mut self, line: &str) -> Result<(), AeiError> {
        if let Some(t) = &mut self.transcript {
            t(Direction::ToEngine, line);
        }
        let mut data = String::with_capacity(line.len() + 1);
        data.push_str(line);
        data.push('\n');
        self.stdin.write_all(data.as_bytes()).await.map_err(AeiError::Io)?;
        self.stdin.flush().await.map_err(AeiError::Io)
    }

    /// Waits for the next message. Errors if the engine exits.
    pub async fn recv(&mut self) -> Result<EngineMessage, AeiError> {
        match self.lines.next_line().await.map_err(AeiError::Io)? {
            Some(line) => {
                if let Some(t) = &mut self.transcript {
                    t(Direction::FromEngine, &line);
                }
                Ok(EngineMessage::parse(&line))
            }
            None => Err(AeiError::Exited),
        }
    }

    /// Waits for the next message until `deadline`; `Ok(None)` on timeout.
    pub async fn recv_until(&mut self, deadline: Instant) -> Result<Option<EngineMessage>, AeiError> {
        match timeout_at(deadline, self.recv()).await {
            Ok(msg) => msg.map(Some),
            Err(_) => Ok(None),
        }
    }

    /// Sends `isready` and waits for `readyok`, returning anything else the
    /// engine sent in between (logs, stray info).
    pub async fn is_ready(&mut self, timeout: Duration) -> Result<Vec<EngineMessage>, AeiError> {
        self.send("isready").await?;
        let deadline = Instant::now() + timeout;
        let mut other = Vec::new();
        loop {
            match self.recv_until(deadline).await? {
                None => return Err(AeiError::Timeout("readyok")),
                Some(EngineMessage::ReadyOk) => return Ok(other),
                Some(msg) => other.push(msg),
            }
        }
    }

    pub async fn new_game(&mut self) -> Result<(), AeiError> {
        self.send("newgame").await
    }

    /// `setposition <side> <board>` with the side to move in `pos`.
    pub async fn set_position(&mut self, pos: &Position) -> Result<(), AeiError> {
        let side = pos.side_to_move().letter();
        self.send(&format!("setposition {side} {}", pos.to_short_string())).await
    }

    pub async fn set_option(&mut self, name: &str, value: impl std::fmt::Display) -> Result<(), AeiError> {
        self.send(&format!("setoption name {name} value {value}")).await
    }

    /// Presses a button option: `setoption name <name>` with no value.
    pub async fn press(&mut self, name: &str) -> Result<(), AeiError> {
        self.send(&format!("setoption name {name}")).await
    }

    /// Sends the time-control options (`tcmove`, `tcreserve`, ...).
    pub async fn set_time_control(&mut self, tc: &TimeControl) -> Result<(), AeiError> {
        self.set_option("tcmove", tc.move_time).await?;
        self.set_option("tcreserve", tc.reserve).await?;
        self.set_option("tcpercent", tc.percent).await?;
        self.set_option("tcmax", tc.max_reserve).await?;
        self.set_option("tcturns", tc.turn_limit).await?;
        self.set_option("tctotal", tc.time_limit).await?;
        self.set_option("tcturntime", tc.max_turn_time).await
    }

    /// Sends the per-turn clock options: both reserves (gold, silver) in
    /// whole seconds, and `moveused` 0. Protocol-version-0 engines also get
    /// the old `wreserve`/`breserve` names.
    pub async fn set_clock(&mut self, reserves: [Duration; 2]) -> Result<(), AeiError> {
        let [g, s] = reserves.map(|r| r.as_secs());
        if self.id.protocol_version == 0 {
            self.set_option("wreserve", g).await?;
            self.set_option("breserve", s).await?;
            self.set_option("tcmoveused", 0).await?;
        }
        self.set_option("greserve", g).await?;
        self.set_option("sreserve", s).await?;
        self.set_option("moveused", 0).await
    }

    /// Tells the engine a move was played (setup placements or steps).
    pub async fn make_move(&mut self, notation: &str) -> Result<(), AeiError> {
        self.send(&format!("makemove {notation}")).await
    }

    pub async fn go(&mut self) -> Result<(), AeiError> {
        self.send("go").await
    }

    pub async fn go_ponder(&mut self) -> Result<(), AeiError> {
        self.send("go ponder").await
    }

    pub async fn stop(&mut self) -> Result<(), AeiError> {
        self.send("stop").await
    }

    /// Sends `quit` and waits up to `timeout` for the process to exit, then
    /// kills it. Either way, whatever else is in its process group (or its
    /// job, on Windows) is killed too, so nothing it started outlives it.
    pub async fn quit(mut self, timeout: Duration) -> Result<(), AeiError> {
        // The engine may already be gone; that's fine. A stuck pipe
        // mustn't hold us past the timeout either.
        let deadline = Instant::now() + timeout;
        let pid = self.child.id();
        let _ = timeout_at(deadline, self.send("quit")).await;
        let exited = timeout_at(deadline, self.child.wait()).await;
        // A process we gave up on hasn't been waited for, so its id can't
        // have been reused. One that exited was waited for just now, and
        // ids are only reused after wrapping around.
        self.kill_tree(pid);
        match exited {
            Ok(status) => status.map(|_| ()).map_err(AeiError::Io),
            Err(_) => self.child.wait().await.map(|_| ()).map_err(AeiError::Io),
        }
    }

    /// Kills the engine and everything else in its group or job, at once.
    pub async fn kill(mut self) -> Result<(), AeiError> {
        self.kill_tree(self.child.id());
        self.child.wait().await.map(|_| ()).map_err(AeiError::Io)
    }

    /// Sends SIGKILL to the engine's process group (or the process, if it
    /// has none of its own), or terminates its job. It doesn't wait.
    fn kill_tree(&mut self, #[cfg_attr(not(unix), allow(unused_variables))] pid: Option<u32>) {
        #[cfg(unix)]
        if self.group
            && let Some(pid) = pid
        {
            process::kill_group(pid);
            return;
        }
        #[cfg(windows)]
        if let Some(job) = &self.job {
            job.terminate();
            return;
        }
        let _ = self.child.start_kill();
    }
}

impl Drop for Engine {
    /// `kill_on_drop` kills only the process; take its group (or job) too.
    /// The id is ours while the child hasn't been waited for.
    fn drop(&mut self) {
        #[cfg(unix)]
        if self.group
            && let Some(pid) = self.child.id()
        {
            process::kill_group(pid);
        }
    }
}
