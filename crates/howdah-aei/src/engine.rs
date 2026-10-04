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

/// How to launch an engine. The program is run directly (no shell).
#[derive(Clone, Debug)]
pub struct EngineConfig {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub working_dir: Option<PathBuf>,
    /// How long to wait for `aeiok` after sending `aei`.
    pub handshake_timeout: Duration,
}

impl EngineConfig {
    pub fn new(program: impl Into<PathBuf>) -> EngineConfig {
        EngineConfig {
            program: program.into(),
            args: Vec::new(),
            working_dir: None,
            handshake_timeout: Duration::from_secs(10),
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
pub struct Engine {
    child: Child,
    stdin: ChildStdin,
    lines: Lines<BufReader<ChildStdout>>,
    id: EngineId,
    transcript: Option<Transcript>,
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
        let mut cmd = Command::new(&config.program);
        cmd.args(&config.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true);
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
        let stdin = child.stdin.take().expect("stdin is piped");
        let stdout = child.stdout.take().expect("stdout is piped");
        let mut engine = Engine {
            child,
            stdin,
            lines: BufReader::new(stdout).lines(),
            id: EngineId::default(),
            transcript,
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
    /// kills it.
    pub async fn quit(mut self, timeout: Duration) -> Result<(), AeiError> {
        // The engine may already be gone; that's fine.
        let _ = self.send("quit").await;
        match tokio::time::timeout(timeout, self.child.wait()).await {
            Ok(status) => status.map(|_| ()).map_err(AeiError::Io),
            Err(_) => self.child.kill().await.map_err(AeiError::Io),
        }
    }
}
