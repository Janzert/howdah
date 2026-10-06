//! The app's command handlers, independent of Tauri. `commands.rs` wraps
//! them as Tauri commands. The dev bridge (`src/bin/dev-bridge.rs`) serves
//! them over HTTP through [`Backend::dispatch`], so the frontend can run in
//! a plain browser against the real session and engines.
//!
//! Commands are intents: mutating commands return only success or an error,
//! and the new state arrives as a `game://changed` event. Queries
//! (`get_state`, `legal_targets`, `export_game`, the engine list) return data.
//!
//! Every state source (UI intents, the engine controller, a followed
//! gameroom game) goes through the same event path, so the frontend has one
//! place where state comes in.
//!
//! The backend holds several sessions, each a game with its own engine
//! controller (a window per game, or a gameroom game being followed). Every
//! session command names its session, and every event from a session
//! carries a `session` field. [`MAIN_SESSION`] always exists: it's the main
//! window's.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use howdah_arimaa::{Color, Glyph, NodeId, Square, TimeControl};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::controller::{self, Controller, SharedRegistry, SharedSession};
use crate::dto::{
    AnimStep, ApiError, EngineIdentity, EngineSpec, GameroomGames, GameroomStatus, MatchSpec, MoveReplay,
    PlayerGamesView, PlayerMatchView, PlayerSpec, PositionView, SessionId, SessionUpdate, SessionView,
    StepTarget, WatchView,
};
use crate::engines::{self, EngineRegistry};
use crate::gameroom::{self, Gameroom, SavedLogin, WATCH_UPDATE, Watch};
use crate::session::{AnalysisEngine, Player, Session};

/// Event carrying a [`SessionUpdate`] after every change to the session.
pub const GAME_CHANGED: &str = "game://changed";

/// Where events go: Tauri's event system in the app, the dev bridge's
/// event stream outside it.
pub trait EventSink: Send + Sync + 'static {
    fn emit(&self, event: &str, payload: Value);
}

pub type Events = Arc<dyn EventSink>;

/// The main window's session. It's opened with the backend and can't be
/// closed. `api.ts` has the same number.
pub const MAIN_SESSION: SessionId = SessionId(1);

/// Adds the session's id to every event it sends, as a `session` field.
struct SessionEvents {
    id: SessionId,
    inner: Events,
}

impl EventSink for SessionEvents {
    fn emit(&self, event: &str, mut payload: Value) {
        if let Value::Object(fields) = &mut payload {
            fields.insert("session".into(), self.id.0.into());
        }
        self.inner.emit(event, payload);
    }
}

/// Serializes `payload` and sends it to `events`.
pub fn emit(events: &Events, event: &str, payload: impl Serialize) {
    match serde_json::to_value(payload) {
        Ok(v) => events.emit(event, v),
        Err(e) => eprintln!("failed to serialize {event}: {e}"),
    }
}

pub fn emit_session(events: &Events, session: &Session, animation: Vec<AnimStep>, budget: Option<Duration>) {
    let update = SessionUpdate {
        view: session.view(),
        animation,
        animation_budget_ms: budget.map(|d| d.as_millis() as u64),
    };
    emit(events, GAME_CHANGED, update);
}

/// One session, the controller running its engines, and the gameroom
/// game it follows, if any.
struct SessionHandle {
    session: SharedSession,
    controller: Controller,
    events: Events,
    watch: Mutex<Option<Watch>>,
}

impl SessionHandle {
    fn lock(&self) -> MutexGuard<'_, Session> {
        // A panic mid-command can't leave the session half-updated in a way
        // that matters more than refusing all further commands would.
        self.session.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn watch(&self) -> MutexGuard<'_, Option<Watch>> {
        self.watch.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Stops following a gameroom game (or forgets an ended one), if the
    /// session has one.
    fn stop_watching(&self) {
        let watch = self.watch().take();
        if let Some(mut watch) = watch {
            emit(&self.events, WATCH_UPDATE, watch.stop());
        }
    }
}

impl Drop for SessionHandle {
    fn drop(&mut self) {
        // Ends the coordinator, which quits the session's engines.
        self.controller.shutdown();
    }
}

pub struct Backend {
    sessions: Mutex<BTreeMap<SessionId, Arc<SessionHandle>>>,
    next_session: AtomicU32,
    engines: SharedRegistry,
    gameroom: Arc<Gameroom>,
    events: Events,
}

impl Backend {
    /// Creates the backend with the main session open. Needs a Tokio
    /// runtime registered with `tauri::async_runtime`.
    pub fn new(registry: EngineRegistry, saved_login: SavedLogin, events: Events) -> Backend {
        let backend = Backend {
            sessions: Mutex::new(BTreeMap::new()),
            next_session: AtomicU32::new(MAIN_SESSION.0),
            engines: Arc::new(Mutex::new(registry)),
            gameroom: Arc::new(Gameroom::new(saved_login)),
            events,
        };
        let main = backend.open_session();
        debug_assert_eq!(main, MAIN_SESSION);
        backend
    }

    fn sessions(&self) -> MutexGuard<'_, BTreeMap<SessionId, Arc<SessionHandle>>> {
        self.sessions.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn engines(&self) -> MutexGuard<'_, EngineRegistry> {
        self.engines.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn handle(&self, id: SessionId) -> Result<Arc<SessionHandle>, ApiError> {
        self.sessions().get(&id).cloned().ok_or_else(|| ApiError::state(format!("no session {id}")))
    }

    /// Reads the session `id`.
    fn read<T>(&self, id: SessionId, f: impl FnOnce(&Session) -> T) -> Result<T, ApiError> {
        Ok(f(&self.handle(id)?.lock()))
    }

    /// Runs a mutation, broadcasts the result, and lets the controller react.
    fn mutate(
        &self,
        id: SessionId,
        f: impl FnOnce(&mut Session) -> Result<Vec<AnimStep>, ApiError>,
    ) -> Result<(), ApiError> {
        let handle = self.handle(id)?;
        {
            let mut session = handle.lock();
            let animation = f(&mut session)?;
            emit_session(&handle.events, &session, animation, None);
        }
        handle.controller.poke();
        if let Some(watch) = handle.watch().as_ref() {
            watch.poke();
        }
        Ok(())
    }

    /// Opens a new session with an empty game, and returns its id.
    pub fn open_session(&self) -> SessionId {
        let id = SessionId(self.next_session.fetch_add(1, Ordering::Relaxed));
        let events: Events = Arc::new(SessionEvents { id, inner: self.events.clone() });
        let session = Arc::new(Mutex::new(Session::new()));
        let controller = controller::spawn(events.clone(), session.clone(), self.engines.clone());
        self.sessions()
            .insert(id, Arc::new(SessionHandle { session, controller, events, watch: Mutex::new(None) }));
        id
    }

    /// Closes a session, quitting its engines. The main session stays.
    pub fn close_session(&self, id: SessionId) -> Result<(), ApiError> {
        if id == MAIN_SESSION {
            return Err(ApiError::state("the main session can't be closed"));
        }
        self.sessions().remove(&id).map(drop).ok_or_else(|| ApiError::state(format!("no session {id}")))
    }

    pub fn list_sessions(&self) -> Vec<SessionId> {
        self.sessions().keys().copied().collect()
    }

    pub fn get_state(&self, id: SessionId) -> Result<SessionView, ApiError> {
        self.read(id, Session::view)
    }

    pub fn new_game(&self, id: SessionId) -> Result<(), ApiError> {
        self.handle(id)?.stop_watching();
        self.mutate(id, |s| {
            s.new_game();
            Ok(Vec::new())
        })
    }

    pub fn load_game(&self, id: SessionId, record: &str) -> Result<(), ApiError> {
        self.handle(id)?.stop_watching();
        self.mutate(id, |s| s.load(record).map(|_| Vec::new()))
    }

    pub fn export_game(&self, id: SessionId, main_line_only: bool) -> Result<String, ApiError> {
        self.read(id, |s| s.export(main_line_only))
    }

    pub fn goto_ply(&self, id: SessionId, ply: usize) -> Result<(), ApiError> {
        self.mutate(id, |s| s.goto(ply))
    }

    pub fn goto_node(&self, id: SessionId, node: NodeId) -> Result<(), ApiError> {
        self.mutate(id, |s| s.goto_node(node))
    }

    pub fn goto_sibling(&self, id: SessionId, offset: isize) -> Result<(), ApiError> {
        self.mutate(id, |s| s.goto_sibling(offset))
    }

    pub fn goto_branch(&self, id: SessionId, forward: bool) -> Result<(), ApiError> {
        self.mutate(id, |s| s.goto_branch(forward))
    }

    pub fn set_comment(&self, id: SessionId, node: NodeId, text: &str) -> Result<(), ApiError> {
        self.mutate(id, |s| s.set_comment(node, text))
    }

    pub fn toggle_glyph(&self, id: SessionId, node: NodeId, glyph: Glyph) -> Result<(), ApiError> {
        self.mutate(id, |s| s.toggle_glyph(node, glyph))
    }

    pub fn toggle_collapsed(&self, id: SessionId, node: NodeId) -> Result<(), ApiError> {
        self.mutate(id, |s| s.toggle_collapsed(node))
    }

    pub fn promote(&self, id: SessionId, node: NodeId) -> Result<(), ApiError> {
        self.mutate(id, |s| s.promote(node))
    }

    pub fn demote(&self, id: SessionId, node: NodeId) -> Result<(), ApiError> {
        self.mutate(id, |s| s.demote(node))
    }

    pub fn make_main_line(&self, id: SessionId, node: NodeId) -> Result<(), ApiError> {
        self.mutate(id, |s| s.make_main_line(node))
    }

    pub fn delete_from(&self, id: SessionId, node: NodeId) -> Result<(), ApiError> {
        self.mutate(id, |s| s.delete_from(node))
    }

    pub fn goto_live(&self, id: SessionId) -> Result<(), ApiError> {
        self.mutate(id, |s| {
            s.goto_live();
            Ok(Vec::new())
        })
    }

    pub fn move_replay(&self, id: SessionId) -> Result<Option<MoveReplay>, ApiError> {
        self.read(id, Session::move_replay)
    }

    pub fn legal_targets(&self, id: SessionId, from: Square) -> Result<Vec<StepTarget>, ApiError> {
        self.read(id, |s| s.legal_targets(from))
    }

    pub fn try_step(&self, id: SessionId, from: Square, to: Square) -> Result<(), ApiError> {
        self.mutate(id, |s| s.try_step(from, to))
    }

    pub fn plan_route(
        &self,
        id: SessionId,
        from: Square,
        to: Square,
        path: &[Square],
    ) -> Result<Option<Vec<Square>>, ApiError> {
        self.read(id, |s| s.plan_route(from, to, path))
    }

    pub fn try_route(
        &self,
        id: SessionId,
        from: Square,
        to: Square,
        path: &[Square],
    ) -> Result<(), ApiError> {
        self.mutate(id, |s| s.try_route(from, to, path))
    }

    pub fn undo_step(&self, id: SessionId) -> Result<(), ApiError> {
        self.mutate(id, |s| s.undo_step())
    }

    pub fn cancel_turn(&self, id: SessionId) -> Result<(), ApiError> {
        self.mutate(id, |s| Ok(s.cancel_turn()))
    }

    /// Ends the turn being entered: plays it in a match on the user's move
    /// (or plays the shown plan's move), unless `plan` keeps it as a plan.
    pub fn commit_turn(&self, id: SessionId, plan: bool) -> Result<(), ApiError> {
        self.mutate(id, |s| s.commit_turn(plan).map(|_| Vec::new()))
    }

    /// Takes back played moves to the last human move (one ply between engines).
    pub fn take_back(&self, id: SessionId) -> Result<(), ApiError> {
        self.mutate(id, |s| s.take_back())
    }

    /// Whether a step after a full turn starts the other side's turn.
    pub fn set_continue_turns(&self, id: SessionId, on: bool) -> Result<(), ApiError> {
        self.mutate(id, |s| {
            s.set_continue_turns(on);
            Ok(Vec::new())
        })
    }

    pub fn setup_swap(&self, id: SessionId, a: Square, b: Square) -> Result<(), ApiError> {
        self.mutate(id, |s| s.setup_swap(a, b))
    }

    pub fn commit_setup(&self, id: SessionId) -> Result<(), ApiError> {
        self.mutate(id, |s| s.commit_setup().map(|_| Vec::new()))
    }

    pub fn start_match(&self, id: SessionId, spec: &MatchSpec) -> Result<(), ApiError> {
        let player = |p: &PlayerSpec| -> Result<Player, ApiError> {
            match p {
                PlayerSpec::Human => Ok(Player::Human),
                PlayerSpec::Engine { engine_id } => {
                    let e = self
                        .engines()
                        .get(engine_id)
                        .ok_or_else(|| ApiError::illegal(format!("unknown engine {engine_id:?}")))?;
                    Ok(Player::Engine { id: e.id, name: e.name })
                }
            }
        };
        let players = [player(&spec.gold)?, player(&spec.silver)?];
        let parse = |tc: &Option<String>| -> Result<Option<TimeControl>, ApiError> {
            match tc.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
                Some(t) => {
                    t.parse().map(Some).map_err(|e| ApiError::illegal(format!("time control {t:?}: {e}")))
                }
                None => Ok(None),
            }
        };
        let tcs = [parse(&spec.gold_time_control)?, parse(&spec.silver_time_control)?];
        self.handle(id)?.stop_watching();
        self.mutate(id, |s| {
            s.start_match(players, tcs, spec.takebacks);
            Ok(Vec::new())
        })
    }

    /// Stops the match (and following a gameroom game); the game stays.
    pub fn end_match(&self, id: SessionId) -> Result<(), ApiError> {
        self.handle(id)?.stop_watching();
        self.mutate(id, |s| {
            s.end_match();
            Ok(Vec::new())
        })
    }

    /// Turns analysis on with the engine `engine_id`, or off with `None`.
    /// (It will be refused while the user plays an online game.)
    pub fn set_analysis(&self, id: SessionId, engine_id: Option<&str>) -> Result<(), ApiError> {
        let engine = match engine_id {
            Some(e) => {
                let spec = self
                    .engines()
                    .get(e)
                    .ok_or_else(|| ApiError::illegal(format!("unknown engine {e:?}")))?;
                Some(AnalysisEngine { id: spec.id, name: spec.name })
            }
            None => None,
        };
        self.mutate(id, |s| {
            s.set_analysis(engine);
            Ok(Vec::new())
        })
    }

    /// Adds moves as a line from `from` and shows its end (see
    /// [`Session::add_line`]).
    pub fn add_line(&self, id: SessionId, from: NodeId, moves: &[String]) -> Result<(), ApiError> {
        self.mutate(id, |s| s.add_line(from, moves))
    }

    pub fn preview_line(
        &self,
        id: SessionId,
        from: NodeId,
        moves: &[String],
    ) -> Result<PositionView, ApiError> {
        self.read(id, |s| s.preview_line(from, moves))?
    }

    pub fn engine_move_now(&self, id: SessionId) -> Result<(), ApiError> {
        self.handle(id)?.controller.move_now();
        Ok(())
    }

    pub fn list_engines(&self) -> Vec<EngineSpec> {
        self.engines().list()
    }

    pub fn save_engine(&self, spec: EngineSpec) -> Result<EngineSpec, ApiError> {
        self.engines().save(spec)
    }

    pub fn delete_engine(&self, id: &str) -> Result<(), ApiError> {
        self.engines().delete(id)
    }

    /// Starts the engine to check it speaks AEI, and reports its identity.
    pub async fn test_engine(spec: &EngineSpec) -> Result<EngineIdentity, ApiError> {
        engines::probe(spec).await
    }

    pub fn gameroom_status(&self) -> GameroomStatus {
        self.gameroom.status()
    }

    /// Logs in to arimaa.com. An empty password uses the saved one; with
    /// `remember` the login is saved (the password obfuscated), and without
    /// it any saved login is forgotten.
    pub async fn gameroom_login(
        &self,
        username: &str,
        password: &str,
        remember: bool,
    ) -> Result<GameroomStatus, ApiError> {
        self.gameroom.login(username, password, remember).await
    }

    pub async fn gameroom_logout(&self) -> Result<(), ApiError> {
        self.gameroom.logout().await
    }

    /// The games being played in the gameroom now, and the last few
    /// finished.
    pub async fn gameroom_games(&self) -> Result<GameroomGames, ApiError> {
        self.gameroom.games().await
    }

    /// The gameroom's players whose username or real name contains `text`.
    pub async fn search_gameroom_players(&self, text: &str) -> Result<Vec<PlayerMatchView>, ApiError> {
        self.gameroom.search_players(text).await
    }

    /// A gameroom player's finished games, newest first, 50 from `offset`.
    pub async fn gameroom_player_games(
        &self,
        player_id: &str,
        offset: u32,
    ) -> Result<PlayerGamesView, ApiError> {
        self.gameroom.player_games(player_id, offset).await
    }

    /// Opens arimaa.com game `gid` in session `id`. A live game (its
    /// gameroom id) is followed as a viewer: the session becomes that game,
    /// and moves, clocks and the result arrive as the server reports them
    /// (see `gameroom.rs`). A finished game (its permanent id) is loaded
    /// whole.
    pub async fn open_gameroom_game(&self, id: SessionId, gid: &str) -> Result<(), ApiError> {
        let handle = self.handle(id)?;
        let target = gameroom::Target {
            session: handle.session.clone(),
            controller: handle.controller.clone(),
            events: handle.events.clone(),
        };
        match gameroom::open(&self.gameroom, gid, target).await? {
            gameroom::Open::Watching(watch) => {
                // An earlier watch's task ends as it's dropped, without a
                // "stopped" event, which would hide the new one.
                let old = handle.watch().replace(watch);
                drop(old);
                Ok(())
            }
            gameroom::Open::Finished(record) => {
                handle.stop_watching();
                self.mutate(id, |s| {
                    s.load_record_from_start(record);
                    Ok(Vec::new())
                })
            }
        }
    }

    /// Plays live arimaa.com game `gid` (its gameroom id) in session `id`
    /// as `side`: the user's seat is taken, the session becomes that game
    /// with the user as `side`'s human player, and the moves played there
    /// are sent to the server (see `gameroom.rs`).
    pub async fn play_gameroom_game(&self, id: SessionId, gid: &str, side: Color) -> Result<(), ApiError> {
        let handle = self.handle(id)?;
        let target = gameroom::Target {
            session: handle.session.clone(),
            controller: handle.controller.clone(),
            events: handle.events.clone(),
        };
        let watch = gameroom::play(&self.gameroom, gid, side, target).await?;
        let old = handle.watch().replace(watch);
        drop(old);
        Ok(())
    }

    /// Stops following the session's gameroom game; the game stays.
    pub fn stop_watching(&self, id: SessionId) -> Result<(), ApiError> {
        self.handle(id)?.stop_watching();
        Ok(())
    }

    /// The session's followed gameroom game, if it has one.
    pub fn watch_status(&self, id: SessionId) -> Result<Option<WatchView>, ApiError> {
        Ok(self.handle(id)?.watch().as_ref().map(Watch::view))
    }

    /// Runs a command by name with JSON arguments, as `invoke(cmd, args)`
    /// does in the app. Used by the dev bridge. Keep it in step with the
    /// handler list in `lib.rs`; a test checks it covers `api.ts`.
    pub async fn dispatch(&self, cmd: &str, args: &Value) -> Result<Value, ApiError> {
        fn ok(v: impl Serialize) -> Result<Value, ApiError> {
            serde_json::to_value(v)
                .map_err(|e| ApiError::state(format!("couldn't serialize the result: {e}")))
        }
        let sid = || arg::<SessionId>(args, "session");
        match cmd {
            "open_session" => ok(self.open_session()),
            "close_session" => ok(self.close_session(sid()?)?),
            "list_sessions" => ok(self.list_sessions()),
            "get_state" => ok(self.get_state(sid()?)?),
            "new_game" => ok(self.new_game(sid()?)?),
            "load_game" => ok(self.load_game(sid()?, &arg::<String>(args, "record")?)?),
            "export_game" => ok(self.export_game(sid()?, arg(args, "mainLineOnly")?)?),
            "goto_ply" => ok(self.goto_ply(sid()?, arg(args, "ply")?)?),
            "goto_live" => ok(self.goto_live(sid()?)?),
            "goto_node" => ok(self.goto_node(sid()?, arg(args, "node")?)?),
            "goto_sibling" => ok(self.goto_sibling(sid()?, arg(args, "offset")?)?),
            "goto_branch" => ok(self.goto_branch(sid()?, arg(args, "forward")?)?),
            "set_comment" => {
                ok(self.set_comment(sid()?, arg(args, "node")?, &arg::<String>(args, "text")?)?)
            }
            "toggle_glyph" => ok(self.toggle_glyph(sid()?, arg(args, "node")?, arg(args, "glyph")?)?),
            "toggle_collapsed" => ok(self.toggle_collapsed(sid()?, arg(args, "node")?)?),
            "promote" => ok(self.promote(sid()?, arg(args, "node")?)?),
            "demote" => ok(self.demote(sid()?, arg(args, "node")?)?),
            "make_main_line" => ok(self.make_main_line(sid()?, arg(args, "node")?)?),
            "delete_from" => ok(self.delete_from(sid()?, arg(args, "node")?)?),
            "move_replay" => ok(self.move_replay(sid()?)?),
            "legal_targets" => ok(self.legal_targets(sid()?, arg(args, "from")?)?),
            "try_step" => ok(self.try_step(sid()?, arg(args, "from")?, arg(args, "to")?)?),
            "plan_route" => ok(self.plan_route(
                sid()?,
                arg(args, "from")?,
                arg(args, "to")?,
                &arg::<Vec<Square>>(args, "path")?,
            )?),
            "try_route" => ok(self.try_route(
                sid()?,
                arg(args, "from")?,
                arg(args, "to")?,
                &arg::<Vec<Square>>(args, "path")?,
            )?),
            "undo_step" => ok(self.undo_step(sid()?)?),
            "cancel_turn" => ok(self.cancel_turn(sid()?)?),
            "commit_turn" => ok(self.commit_turn(sid()?, arg(args, "plan")?)?),
            "take_back" => ok(self.take_back(sid()?)?),
            "set_continue_turns" => ok(self.set_continue_turns(sid()?, arg(args, "on")?)?),
            "setup_swap" => ok(self.setup_swap(sid()?, arg(args, "a")?, arg(args, "b")?)?),
            "commit_setup" => ok(self.commit_setup(sid()?)?),
            "start_match" => ok(self.start_match(sid()?, &arg(args, "spec")?)?),
            "end_match" => ok(self.end_match(sid()?)?),
            "engine_move_now" => ok(self.engine_move_now(sid()?)?),
            "set_analysis" => {
                ok(self.set_analysis(sid()?, arg::<Option<String>>(args, "engineId")?.as_deref())?)
            }
            "add_line" => {
                ok(self.add_line(sid()?, arg(args, "from")?, &arg::<Vec<String>>(args, "moves")?)?)
            }
            "preview_line" => {
                ok(self.preview_line(sid()?, arg(args, "from")?, &arg::<Vec<String>>(args, "moves")?)?)
            }
            "list_engines" => ok(self.list_engines()),
            "save_engine" => ok(self.save_engine(arg(args, "spec")?)?),
            "delete_engine" => ok(self.delete_engine(&arg::<String>(args, "id")?)?),
            "test_engine" => ok(Backend::test_engine(&arg(args, "spec")?).await?),
            "gameroom_status" => ok(self.gameroom_status()),
            "gameroom_login" => ok(self
                .gameroom_login(
                    &arg::<String>(args, "username")?,
                    &arg::<String>(args, "password")?,
                    arg(args, "remember")?,
                )
                .await?),
            "gameroom_logout" => ok(self.gameroom_logout().await?),
            "gameroom_games" => ok(self.gameroom_games().await?),
            "search_gameroom_players" => {
                ok(self.search_gameroom_players(&arg::<String>(args, "text")?).await?)
            }
            "gameroom_player_games" => ok(self
                .gameroom_player_games(&arg::<String>(args, "playerId")?, arg(args, "offset")?)
                .await?),
            "open_gameroom_game" => ok(self.open_gameroom_game(sid()?, &arg::<String>(args, "gid")?).await?),
            "play_gameroom_game" => {
                ok(self.play_gameroom_game(sid()?, &arg::<String>(args, "gid")?, arg(args, "side")?).await?)
            }
            "stop_watching" => ok(self.stop_watching(sid()?)?),
            "watch_status" => ok(self.watch_status(sid()?)?),
            _ => Err(ApiError::state(format!("unknown command {cmd:?}"))),
        }
    }
}

/// Reads one named argument, as Tauri does for a command parameter.
fn arg<T: DeserializeOwned>(args: &Value, name: &str) -> Result<T, ApiError> {
    let v = args.get(name).cloned().unwrap_or(Value::Null);
    serde_json::from_value(v).map_err(|e| ApiError::state(format!("argument {name:?}: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NullSink;
    impl EventSink for NullSink {
        fn emit(&self, _: &str, _: Value) {}
    }

    /// Keeps the events it's sent.
    #[derive(Default)]
    struct Recorder(Mutex<Vec<(String, Value)>>);
    impl EventSink for Recorder {
        fn emit(&self, event: &str, payload: Value) {
            self.0.lock().unwrap().push((event.to_string(), payload));
        }
    }

    /// One runtime for every test: Tauri's can only be set once, and the
    /// controllers keep running on it after a test's own runtime would end.
    fn runtime() -> &'static tokio::runtime::Runtime {
        static RT: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();
        RT.get_or_init(|| {
            let rt = tokio::runtime::Runtime::new().expect("a runtime");
            tauri::async_runtime::set(rt.handle().clone());
            rt
        })
    }

    fn backend(events: Events) -> Backend {
        runtime();
        let path = std::env::temp_dir().join(format!("arimaa-backend-test-{}.json", std::process::id()));
        Backend::new(EngineRegistry::load(path), SavedLogin::new(None), events)
    }

    #[test]
    fn sessions_are_separate_and_tag_their_events() {
        let recorder = Arc::new(Recorder::default());
        let b = backend(recorder.clone());
        assert_eq!(b.list_sessions(), [MAIN_SESSION]);
        let other = b.open_session();
        assert_ne!(other, MAIN_SESSION);
        b.setup_swap(other, "a1".parse().unwrap(), "e2".parse().unwrap()).unwrap();
        b.commit_setup(other).unwrap();
        assert_eq!(b.get_state(other).unwrap().ply, 1);
        assert_eq!(b.get_state(MAIN_SESSION).unwrap().ply, 0, "the main session is untouched");
        let events = recorder.0.lock().unwrap().clone();
        assert!(!events.is_empty());
        for (name, payload) in &events {
            assert_eq!(name, GAME_CHANGED);
            assert_eq!(payload["session"], other.0, "every event names its session");
        }
        assert!(b.close_session(MAIN_SESSION).is_err());
        b.close_session(other).unwrap();
        assert_eq!(b.list_sessions(), [MAIN_SESSION]);
        let e = b.get_state(other).unwrap_err();
        assert_eq!(e.message, format!("no session {other}"));
        assert!(b.close_session(other).is_err());
    }

    /// Every command `api.ts` invokes must be handled by `dispatch`, or the
    /// dev bridge silently falls behind the app.
    #[test]
    fn dispatch_covers_api_ts() {
        let backend = backend(Arc::new(NullSink));
        let api = include_str!("../../src/lib/api.ts");
        let names: Vec<&str> = api.split("invoke<").skip(1).filter_map(|s| s.split('\'').nth(1)).collect();
        assert!(names.len() > 10, "found only {names:?} in api.ts");
        let main = format!("MAIN_SESSION = {MAIN_SESSION}");
        assert!(api.contains(&main), "api.ts should say {main}, as backend.rs does");
        for name in names {
            if let Err(e) = runtime().block_on(backend.dispatch(name, &Value::Null)) {
                assert!(!e.message.starts_with("unknown command"), "dispatch doesn't handle {name}");
            }
        }
    }
}
