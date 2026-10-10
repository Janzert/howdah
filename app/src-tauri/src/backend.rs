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
//! carries a `session` field. Each session is a game window's; the main
//! window (the lobby) has none.

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
    AnimStep, ApiError, BotInfoView, EngineCatalogView, EngineIdentity, EngineOption, EngineSpec,
    GameroomGames, GameroomStatus, MatchSpec, MoveReplay, PlayerGamesView, PlayerMatchView, PlayerSpec,
    PositionCheck, PositionProblemView, PositionSpec, PositionView, PostalGameView, ServerBotsView,
    SessionId, SessionUpdate, SessionView, SessionsChanged, StartedBotView, StepTarget, WatchView,
};
use crate::engine_install::{self, EngineCatalog};
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
/// Event carrying [`SessionsChanged`] when a session opens or closes.
pub const SESSIONS_CHANGED: &str = "sessions://changed";

/// The session a game window shows, from its label (`game-<session>`, as
/// the frontend names the windows it opens; `lib/windows.ts`).
pub fn game_window_session(label: &str) -> Option<SessionId> {
    label.strip_prefix("game-")?.parse().ok().map(SessionId)
}

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
    /// Engine manifests and installing from them.
    catalog: Arc<Mutex<EngineCatalog>>,
    gameroom: Arc<Gameroom>,
    events: Events,
}

impl Backend {
    /// Creates the backend, with no sessions open. Needs a Tokio runtime
    /// registered with `tauri::async_runtime`.
    pub fn new(
        registry: EngineRegistry,
        catalog: EngineCatalog,
        saved_login: SavedLogin,
        events: Events,
    ) -> Backend {
        Backend {
            sessions: Mutex::new(BTreeMap::new()),
            next_session: AtomicU32::new(1),
            engines: Arc::new(Mutex::new(registry)),
            catalog: Arc::new(Mutex::new(catalog)),
            gameroom: Arc::new(Gameroom::new(saved_login, events.clone())),
            events,
        }
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
        self.start_session(id);
        self.sessions_changed();
        id
    }

    /// Tells every window which sessions are open.
    fn sessions_changed(&self) {
        emit(&self.events, SESSIONS_CHANGED, SessionsChanged { sessions: self.list_sessions() });
    }

    /// Starts session `id` with an empty game.
    fn start_session(&self, id: SessionId) {
        let events: Events = Arc::new(SessionEvents { id, inner: self.events.clone() });
        let session = Arc::new(Mutex::new(Session::new()));
        let controller = controller::spawn(events.clone(), session.clone(), self.engines.clone());
        self.sessions()
            .insert(id, Arc::new(SessionHandle { session, controller, events, watch: Mutex::new(None) }));
    }

    /// Closes a session, quitting its engines and leaving any arimaa.com
    /// game it follows (the game goes on there).
    pub fn close_session(&self, id: SessionId) -> Result<(), ApiError> {
        let handle =
            self.sessions().remove(&id).ok_or_else(|| ApiError::state(format!("no session {id}")))?;
        handle.stop_watching();
        self.sessions_changed();
        Ok(())
    }

    /// A window was destroyed (`label` is its Tauri label). A game window's
    /// session ends with it, as [`close_session`](Self::close_session)
    /// does, unless the frontend closed it already.
    pub fn window_closed(&self, label: &str) {
        if let Some(id) = game_window_session(label) {
            let _ = self.close_session(id);
        }
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

    /// A new game in the session from a set position (the position
    /// editor's Analyse).
    pub fn new_game_from(&self, id: SessionId, position: &PositionSpec) -> Result<(), ApiError> {
        let start = position.to_start()?;
        self.handle(id)?.stop_watching();
        self.mutate(id, |s| s.new_game_from(&start).map(|_| Vec::new()))
    }

    /// The position the session shows (before any turn being entered), for
    /// the position editor. During the setups it's the pieces placed so
    /// far, with gold to move at `2g`.
    pub fn editor_position(&self, id: SessionId) -> Result<PositionSpec, ApiError> {
        self.read(id, |s| PositionSpec::from_start(&s.shown_start()))
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

    /// Asks the opponent in a game on a server to take back the user's last
    /// move.
    pub fn request_takeback(&self, id: SessionId) -> Result<(), ApiError> {
        self.mutate(id, |s| s.request_takeback().map(|_| Vec::new()))
    }

    /// Answers the opponent's takeback request in a game on a server.
    pub fn answer_takeback(&self, id: SessionId, accept: bool) -> Result<(), ApiError> {
        self.mutate(id, |s| s.answer_takeback(accept).map(|_| Vec::new()))
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
                PlayerSpec::Engine { engine_id, options } => {
                    let e = self
                        .engines()
                        .get(engine_id)
                        .ok_or_else(|| ApiError::illegal(format!("unknown engine {engine_id:?}")))?;
                    Ok(Player::Engine {
                        id: e.id,
                        name: e.name,
                        options: options.clone().unwrap_or_default(),
                    })
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
        let start = spec.start.as_ref().map(PositionSpec::to_start).transpose()?;
        self.handle(id)?.stop_watching();
        self.mutate(id, |s| {
            s.start_match_from(players, tcs, spec.takebacks, start.as_ref()).map(|_| Vec::new())
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
    /// Turning it on is refused while the user plays an online game.
    pub fn set_analysis(&self, id: SessionId, engine_id: Option<&str>) -> Result<(), ApiError> {
        let engine = match engine_id {
            Some(e) => {
                let spec = self
                    .engines()
                    .get(e)
                    .ok_or_else(|| ApiError::illegal(format!("unknown engine {e:?}")))?;
                Some(AnalysisEngine { id: spec.id, name: spec.name, options: Vec::new() })
            }
            None => None,
        };
        self.mutate(id, |s| {
            s.set_analysis(engine)?;
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

    /// Sets the options for this game of the engine playing `side`.
    pub fn set_engine_options(
        &self,
        id: SessionId,
        side: Color,
        options: Vec<EngineOption>,
    ) -> Result<(), ApiError> {
        self.mutate(id, |s| {
            s.set_engine_options(side, options)?;
            Ok(Vec::new())
        })
    }

    /// Sets the analysis engine's options for this session.
    pub fn set_analysis_options(&self, id: SessionId, options: Vec<EngineOption>) -> Result<(), ApiError> {
        self.mutate(id, |s| {
            s.set_analysis_options(options)?;
            Ok(Vec::new())
        })
    }

    /// Presses the button option `name` of the engine playing `side`, or of
    /// the analysis engine with `None`.
    pub fn press_engine_button(
        &self,
        id: SessionId,
        side: Option<Color>,
        name: String,
    ) -> Result<(), ApiError> {
        if name.is_empty() || name.contains(char::is_whitespace) {
            return Err(ApiError::illegal(format!("not an option name: {name:?}")));
        }
        self.handle(id)?.controller.press(side, name);
        Ok(())
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

    /// Removes an engine; one installed from a manifest takes its
    /// downloaded files with it.
    pub fn delete_engine(&self, id: &str) -> Result<(), ApiError> {
        let mut engines = self.engines();
        let gone = engines.get(id);
        engines.delete(id)?;
        let remaining = engines.list();
        drop(engines);
        match gone {
            Some(spec) => self.catalog().remove_files(&spec, &remaining),
            None => Ok(()),
        }
    }

    /// The engine manifests added and suggested, with what's installed.
    pub fn engine_catalog(&self) -> EngineCatalogView {
        let engines = self.engines().list();
        self.catalog().view(&engines)
    }

    fn catalog(&self) -> MutexGuard<'_, EngineCatalog> {
        self.catalog.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Adds an engine manifest from a URL (fetched) or a file's text,
    /// replacing one with the same id.
    pub async fn add_engine_manifest(
        &self,
        url: Option<&str>,
        text: Option<&str>,
    ) -> Result<EngineCatalogView, ApiError> {
        match (url.filter(|u| !u.trim().is_empty()), text) {
            (Some(url), _) => {
                engine_install::add_url(&self.catalog, url).await?;
            }
            (None, Some(text)) => {
                self.catalog().add_text(text)?;
            }
            (None, None) => return Err(ApiError::illegal("give a manifest's address or file")),
        }
        Ok(self.engine_catalog())
    }

    /// Forgets an engine manifest; an engine installed from it stays.
    pub fn remove_engine_manifest(&self, id: &str) -> Result<EngineCatalogView, ApiError> {
        self.catalog().remove(id)?;
        Ok(self.engine_catalog())
    }

    /// Fetches a manifest's newest release (nothing is installed).
    pub async fn refresh_engine_manifest(&self, id: &str) -> Result<EngineCatalogView, ApiError> {
        engine_install::refresh(&self.catalog, id).await?;
        Ok(self.engine_catalog())
    }

    /// Downloads and installs a manifest's release for this computer, and
    /// adds the engine (or updates the one installed from it before).
    pub async fn install_engine(&self, id: &str) -> Result<EngineSpec, ApiError> {
        engine_install::install(&self.catalog, &self.engines, id).await
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
        let status = self.gameroom.login(username, password, remember).await?;
        self.gameroom.watch_lobby();
        Ok(status)
    }

    /// Invites `who` to a game with the user as `side`; the answer comes as
    /// `gameroom://invitation`.
    pub async fn invite_gameroom_player(
        &self,
        who: &str,
        side: Color,
        time_control: &str,
        rated: bool,
        message: &str,
    ) -> Result<(), ApiError> {
        self.gameroom.invite(who, side, time_control, rated, message).await
    }

    /// Accepts an invitation (the inviter's id and its time) and plays the
    /// game it makes in session `id`.
    pub async fn accept_gameroom_invitation(
        &self,
        id: SessionId,
        other_id: &str,
        created: &str,
    ) -> Result<(), ApiError> {
        let handle = self.handle(id)?;
        let target = gameroom::Target {
            session: handle.session.clone(),
            controller: handle.controller.clone(),
            events: handle.events.clone(),
        };
        let watch = gameroom::accept(&self.gameroom, other_id, created, target).await?;
        let old = handle.watch().replace(watch);
        drop(old);
        Ok(())
    }

    /// Declines an invitation (the inviter's id and its time).
    pub async fn decline_gameroom_invitation(
        &self,
        other_id: &str,
        created: &str,
        reason: &str,
    ) -> Result<(), ApiError> {
        self.gameroom.decline_invite(other_id, created, reason).await
    }

    /// Cancels the user's invitation (the invited player's id and its time).
    pub async fn cancel_gameroom_invitation(&self, other_id: &str, created: &str) -> Result<(), ApiError> {
        self.gameroom.cancel_invite(other_id, created).await
    }

    pub async fn gameroom_logout(&self) -> Result<(), ApiError> {
        self.gameroom.logout().await
    }

    /// The games being played in the gameroom now, and the last few
    /// finished.
    pub async fn gameroom_games(&self) -> Result<GameroomGames, ApiError> {
        self.gameroom.games().await
    }

    /// The gameroom's lists as last fetched, without asking the server.
    pub fn gameroom_last_games(&self) -> Option<GameroomGames> {
        self.gameroom.last_games()
    }

    /// The gameroom's players whose username or real name contains `text`.
    pub async fn search_gameroom_players(&self, text: &str) -> Result<Vec<PlayerMatchView>, ApiError> {
        self.gameroom.search_players(text).await
    }

    /// The postal games being played in the gameroom.
    pub async fn gameroom_postal_games(&self) -> Result<Vec<PostalGameView>, ApiError> {
        self.gameroom.postal_games().await
    }

    /// The bots arimaa.com runs, with the user's record against each.
    pub async fn gameroom_server_bots(&self) -> Result<ServerBotsView, ApiError> {
        self.gameroom.server_bots().await
    }

    /// What a server bot's page (`ServerBotView.page`) says about it.
    pub async fn gameroom_bot_info(&self, page: &str) -> Result<BotInfoView, ApiError> {
        self.gameroom.bot_info(page).await
    }

    /// Starts server bot `name` (its page `page`) playing `bot_side`, and
    /// waits for the game it opens (see `Gameroom::start_bot`). The user
    /// joins it with [`Backend::play_gameroom_game`].
    pub async fn start_gameroom_bot(
        &self,
        page: &str,
        name: &str,
        bot_side: Color,
    ) -> Result<StartedBotView, ApiError> {
        self.gameroom.start_bot(page, name, bot_side).await
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
    /// are sent to the server (see `gameroom.rs`). `unrated` sits in the
    /// gameroom's unrated mode, which makes a rated game against a bot
    /// unrated.
    pub async fn play_gameroom_game(
        &self,
        id: SessionId,
        gid: &str,
        side: Color,
        unrated: bool,
    ) -> Result<(), ApiError> {
        let handle = self.handle(id)?;
        let target = gameroom::Target {
            session: handle.session.clone(),
            controller: handle.controller.clone(),
            events: handle.events.clone(),
        };
        let watch = gameroom::play_with(&self.gameroom, gid, side, unrated, target).await?;
        let old = handle.watch().replace(watch);
        drop(old);
        Ok(())
    }

    /// Creates an arimaa.com game with the user as `side` and plays it in
    /// session `id`, as [`Backend::play_gameroom_game`] does. Until an
    /// opponent sits, the user's first move is held (`WatchView.waiting`).
    pub async fn create_gameroom_game(
        &self,
        id: SessionId,
        side: Color,
        time_control: &str,
        rated: bool,
    ) -> Result<(), ApiError> {
        let handle = self.handle(id)?;
        let target = gameroom::Target {
            session: handle.session.clone(),
            controller: handle.controller.clone(),
            events: handle.events.clone(),
        };
        let watch = gameroom::create(&self.gameroom, side, time_control, rated, target).await?;
        let old = handle.watch().replace(watch);
        drop(old);
        Ok(())
    }

    /// Cancels arimaa.com game `gid`, which the user created and nobody
    /// has joined. A session playing it starts a new game, since nothing
    /// was played.
    pub async fn cancel_gameroom_game(&self, gid: &str) -> Result<(), ApiError> {
        self.gameroom.cancel_game(gid).await?;
        let handles: Vec<_> = self.sessions().iter().map(|(id, h)| (*id, h.clone())).collect();
        for (id, handle) in handles {
            let playing = handle.watch().as_ref().is_some_and(|w| w.view().gid == gid.trim());
            if playing {
                self.new_game(id)?;
            }
        }
        Ok(())
    }

    /// Resigns the arimaa.com game session `id` plays.
    pub async fn resign_gameroom_game(&self, id: SessionId) -> Result<(), ApiError> {
        let request = match self.handle(id)?.watch().as_ref() {
            Some(watch) => watch.resign()?,
            None => return Err(ApiError::state("no arimaa.com game here")),
        };
        request.await
    }

    /// Sends `text` to the chat of the arimaa.com game session `id` plays.
    pub async fn send_gameroom_chat(&self, id: SessionId, text: &str) -> Result<(), ApiError> {
        let request = match self.handle(id)?.watch().as_ref() {
            Some(watch) => watch.chat(text)?,
            None => return Err(ApiError::state("no arimaa.com game here")),
        };
        request.await
    }

    /// Stops following the session's gameroom game; the game stays.
    pub fn stop_watching(&self, id: SessionId) -> Result<(), ApiError> {
        self.handle(id)?.stop_watching();
        Ok(())
    }

    /// The session following arimaa.com game `gid` (its gameroom id), if
    /// one does, so a game gets only one window.
    pub fn gameroom_game_session(&self, gid: &str) -> Option<SessionId> {
        let gid = gid.trim();
        let handles: Vec<_> = self.sessions().iter().map(|(id, h)| (*id, h.clone())).collect();
        handles
            .into_iter()
            .find(|(_, h)| h.watch().as_ref().is_some_and(|w| w.view().gid == gid))
            .map(|(id, _)| id)
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
            "new_game_from" => ok(self.new_game_from(sid()?, &arg(args, "position")?)?),
            "editor_position" => ok(self.editor_position(sid()?)?),
            "parse_position" => ok(parse_position(&arg::<String>(args, "text")?)?),
            "check_position" => ok(check_position(&arg(args, "position")?)?),
            "edit_position" => ok(edit_position(&arg(args, "position")?, &arg::<String>(args, "text")?)?),
            "default_position" => ok(default_position()),
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
            "request_takeback" => ok(self.request_takeback(sid()?)?),
            "answer_takeback" => ok(self.answer_takeback(sid()?, arg(args, "accept")?)?),
            "set_continue_turns" => ok(self.set_continue_turns(sid()?, arg(args, "on")?)?),
            "setup_swap" => ok(self.setup_swap(sid()?, arg(args, "a")?, arg(args, "b")?)?),
            "commit_setup" => ok(self.commit_setup(sid()?)?),
            "start_match" => ok(self.start_match(sid()?, &arg(args, "spec")?)?),
            "end_match" => ok(self.end_match(sid()?)?),
            "engine_move_now" => ok(self.engine_move_now(sid()?)?),
            "set_engine_options" => {
                ok(self.set_engine_options(sid()?, arg(args, "side")?, arg(args, "options")?)?)
            }
            "set_analysis_options" => ok(self.set_analysis_options(sid()?, arg(args, "options")?)?),
            "press_engine_button" => {
                ok(self.press_engine_button(sid()?, arg(args, "side")?, arg(args, "name")?)?)
            }
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
            "engine_catalog" => ok(self.engine_catalog()),
            "add_engine_manifest" => ok(self
                .add_engine_manifest(
                    arg::<Option<String>>(args, "url")?.as_deref(),
                    arg::<Option<String>>(args, "text")?.as_deref(),
                )
                .await?),
            "remove_engine_manifest" => ok(self.remove_engine_manifest(&arg::<String>(args, "id")?)?),
            "refresh_engine_manifest" => {
                ok(self.refresh_engine_manifest(&arg::<String>(args, "id")?).await?)
            }
            "install_engine" => ok(self.install_engine(&arg::<String>(args, "id")?).await?),
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
            "gameroom_last_games" => ok(self.gameroom_last_games()),
            "gameroom_game_session" => ok(self.gameroom_game_session(&arg::<String>(args, "gid")?)),
            "gameroom_postal_games" => ok(self.gameroom_postal_games().await?),
            "invite_gameroom_player" => ok(self
                .invite_gameroom_player(
                    &arg::<String>(args, "who")?,
                    arg(args, "side")?,
                    &arg::<String>(args, "timeControl")?,
                    arg(args, "rated")?,
                    &arg::<String>(args, "message")?,
                )
                .await?),
            "accept_gameroom_invitation" => ok(self
                .accept_gameroom_invitation(
                    sid()?,
                    &arg::<String>(args, "otherId")?,
                    &arg::<String>(args, "created")?,
                )
                .await?),
            "decline_gameroom_invitation" => ok(self
                .decline_gameroom_invitation(
                    &arg::<String>(args, "otherId")?,
                    &arg::<String>(args, "created")?,
                    &arg::<String>(args, "reason")?,
                )
                .await?),
            "cancel_gameroom_invitation" => ok(self
                .cancel_gameroom_invitation(
                    &arg::<String>(args, "otherId")?,
                    &arg::<String>(args, "created")?,
                )
                .await?),
            "search_gameroom_players" => {
                ok(self.search_gameroom_players(&arg::<String>(args, "text")?).await?)
            }
            "gameroom_player_games" => ok(self
                .gameroom_player_games(&arg::<String>(args, "playerId")?, arg(args, "offset")?)
                .await?),
            "open_gameroom_game" => ok(self.open_gameroom_game(sid()?, &arg::<String>(args, "gid")?).await?),
            "play_gameroom_game" => ok(self
                .play_gameroom_game(
                    sid()?,
                    &arg::<String>(args, "gid")?,
                    arg(args, "side")?,
                    arg::<Option<bool>>(args, "unrated")?.unwrap_or(false),
                )
                .await?),
            "gameroom_server_bots" => ok(self.gameroom_server_bots().await?),
            "gameroom_bot_info" => ok(self.gameroom_bot_info(&arg::<String>(args, "page")?).await?),
            "start_gameroom_bot" => ok(self
                .start_gameroom_bot(
                    &arg::<String>(args, "page")?,
                    &arg::<String>(args, "name")?,
                    arg(args, "botSide")?,
                )
                .await?),
            "create_gameroom_game" => ok(self
                .create_gameroom_game(
                    sid()?,
                    arg(args, "side")?,
                    &arg::<String>(args, "timeControl")?,
                    arg(args, "rated")?,
                )
                .await?),
            "cancel_gameroom_game" => ok(self.cancel_gameroom_game(&arg::<String>(args, "gid")?).await?),
            "resign_gameroom_game" => ok(self.resign_gameroom_game(sid()?).await?),
            "send_gameroom_chat" => {
                ok(self.send_gameroom_chat(sid()?, &arg::<String>(args, "text")?).await?)
            }
            "stop_watching" => ok(self.stop_watching(sid()?)?),
            "watch_status" => ok(self.watch_status(sid()?)?),
            // A browser has no native widgets to match.
            "set_native_appearance" => ok(()),
            _ => Err(ApiError::state(format!("unknown command {cmd:?}"))),
        }
    }
}

/// Reads one named argument, as Tauri does for a command parameter.
/// Reads a position in the short or long format (position editor import).
pub fn parse_position(text: &str) -> Result<PositionSpec, ApiError> {
    let start = howdah_arimaa::StartPosition::parse(text).map_err(|e| ApiError::illegal(e.to_string()))?;
    Ok(PositionSpec::from_start(&start))
}

/// What keeps a position from starting a game, and the position written
/// out, for the position editor.
pub fn check_position(position: &PositionSpec) -> Result<PositionCheck, ApiError> {
    use howdah_arimaa::{Piece, PieceKind, check_start_position};
    let start = position.to_start()?;
    let p = &start.position;
    let problems = check_start_position(p)
        .iter()
        .map(|problem| PositionProblemView { message: problem.to_string(), squares: problem.squares(p) })
        .collect();
    let left = Color::ALL
        .into_iter()
        .flat_map(|color| PieceKind::ALL.into_iter().rev().map(move |kind| Piece::new(color, kind)))
        .map(|piece| (piece.letter(), piece.kind.initial_count() as i32 - p.count(piece) as i32))
        .collect();
    Ok(PositionCheck {
        problems,
        short: start.to_short_string(),
        long: start.to_long_string(),
        label: start.label(),
        left,
    })
}

/// Applies typed edits (`Ra1 Ra1n Ra1x s`) to a position.
pub fn edit_position(position: &PositionSpec, text: &str) -> Result<PositionSpec, ApiError> {
    let mut start = position.to_start()?;
    start.apply_edits(text).map_err(|e| ApiError::illegal(e.to_string()))?;
    Ok(PositionSpec::from_start(&start))
}

/// Both default setups, gold to move at `2g`: the editor's starting position.
pub fn default_position() -> PositionSpec {
    PositionSpec::from_start(&howdah_arimaa::StartPosition::new(howdah_arimaa::default_start()))
}

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
        let catalog =
            EngineCatalog::load(path.with_extension("manifests.json"), path.with_extension("engines"));
        Backend::new(EngineRegistry::load(path), catalog, SavedLogin::new(None), events)
    }

    #[test]
    fn sessions_are_separate_and_tag_their_events() {
        let recorder = Arc::new(Recorder::default());
        let b = backend(recorder.clone());
        assert_eq!(b.list_sessions(), []);
        let one = b.open_session();
        let other = b.open_session();
        assert_ne!(one, other);
        recorder.0.lock().unwrap().clear();
        b.setup_swap(other, "a1".parse().unwrap(), "e2".parse().unwrap()).unwrap();
        b.commit_setup(other).unwrap();
        assert_eq!(b.get_state(other).unwrap().ply, 1);
        assert_eq!(b.get_state(one).unwrap().ply, 0, "the other session is untouched");
        let events = recorder.0.lock().unwrap().clone();
        assert!(!events.is_empty());
        for (name, payload) in &events {
            assert_eq!(name, GAME_CHANGED);
            assert_eq!(payload["session"], other.0, "every event names its session");
        }
        assert_eq!(b.gameroom_game_session("539546"), None, "no session follows an arimaa.com game");
        b.close_session(other).unwrap();
        assert_eq!(b.list_sessions(), [one]);
        let e = b.get_state(other).unwrap_err();
        assert_eq!(e.message, format!("no session {other}"));
        assert!(b.close_session(other).is_err());
    }

    #[test]
    fn opening_and_closing_sessions_says_which_are_open() {
        let recorder = Arc::new(Recorder::default());
        let b = backend(recorder.clone());
        let one = b.open_session();
        let two = b.open_session();
        b.close_session(one).unwrap();
        let events = recorder.0.lock().unwrap().clone();
        let lists: Vec<_> = events
            .iter()
            .filter(|(name, _)| name == SESSIONS_CHANGED)
            .map(|(_, p)| p["sessions"].clone())
            .collect();
        assert_eq!(
            lists,
            [serde_json::json!([one.0]), serde_json::json!([one.0, two.0]), serde_json::json!([two.0])]
        );
    }

    #[test]
    fn closing_a_window_ends_its_session() {
        let b = backend(Arc::new(Recorder::default()));
        let game = b.open_session();
        let kept = b.open_session();
        b.window_closed(&format!("game-{game}"));
        assert_eq!(b.list_sessions(), [kept]);
        b.window_closed(&format!("game-{game}")); // already gone: nothing happens
        b.window_closed("main");
        assert_eq!(b.list_sessions(), [kept]);
    }

    #[test]
    fn game_window_labels_name_their_session() {
        assert_eq!(game_window_session("game-7"), Some(SessionId(7)));
        assert_eq!(game_window_session("main"), None);
        assert_eq!(game_window_session("game-"), None);
        assert_eq!(game_window_session("game-x"), None);
    }

    /// Every command `api.ts` invokes must be handled by `dispatch`, or the
    /// dev bridge silently falls behind the app.
    #[test]
    fn dispatch_covers_api_ts() {
        let backend = backend(Arc::new(NullSink));
        let api = include_str!("../../src/lib/api.ts");
        let names: Vec<&str> = api.split("invoke<").skip(1).filter_map(|s| s.split('\'').nth(1)).collect();
        assert!(names.len() > 10, "found only {names:?} in api.ts");
        for name in names {
            if let Err(e) = runtime().block_on(backend.dispatch(name, &Value::Null)) {
                assert!(!e.message.starts_with("unknown command"), "dispatch doesn't handle {name}");
            }
        }
    }
}
