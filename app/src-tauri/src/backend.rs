//! The app's command handlers, independent of Tauri. `commands.rs` wraps
//! them as Tauri commands. The dev bridge (`src/bin/dev-bridge.rs`) serves
//! them over HTTP through [`Backend::dispatch`], so the frontend can run in
//! a plain browser against the real session and engines.
//!
//! Commands are intents: mutating commands return only success or an error,
//! and the new state arrives as a `game://changed` event. Queries
//! (`get_state`, `legal_targets`, `export_game`, the engine list) return data.
//!
//! Every state source (UI intents, the engine controller, later the
//! gameroom) goes through the same event path, so the frontend has one place
//! where state comes in.

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use howdah_arimaa::{Glyph, NodeId, Square, TimeControl};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::controller::{self, Controller, SharedRegistry, SharedSession};
use crate::dto::{
    AnimStep, ApiError, EngineIdentity, EngineSpec, MatchSpec, MoveReplay, PlayerSpec, PositionView,
    SessionUpdate, SessionView, StepTarget,
};
use crate::engines::{self, EngineRegistry};
use crate::session::{AnalysisEngine, Player, Session};

/// Event carrying a [`SessionUpdate`] after every change to the session.
pub const GAME_CHANGED: &str = "game://changed";

/// Where events go: Tauri's event system in the app, the dev bridge's
/// event stream outside it.
pub trait EventSink: Send + Sync + 'static {
    fn emit(&self, event: &str, payload: Value);
}

pub type Events = Arc<dyn EventSink>;

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

pub struct Backend {
    session: SharedSession,
    engines: SharedRegistry,
    controller: Controller,
    events: Events,
}

impl Backend {
    /// Creates the session and starts the engine controller. Needs a Tokio
    /// runtime registered with `tauri::async_runtime`.
    pub fn new(registry: EngineRegistry, events: Events) -> Backend {
        let engines = Arc::new(Mutex::new(registry));
        let session = Arc::new(Mutex::new(Session::new()));
        let controller = controller::spawn(events.clone(), session.clone(), engines.clone());
        Backend { session, engines, controller, events }
    }

    fn lock(&self) -> MutexGuard<'_, Session> {
        // A panic mid-command can't leave the session half-updated in a way
        // that matters more than refusing all further commands would.
        self.session.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn engines(&self) -> MutexGuard<'_, EngineRegistry> {
        self.engines.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Runs a mutation, broadcasts the result, and lets the controller react.
    fn mutate(
        &self,
        f: impl FnOnce(&mut Session) -> Result<Vec<AnimStep>, ApiError>,
    ) -> Result<(), ApiError> {
        {
            let mut session = self.lock();
            let animation = f(&mut session)?;
            emit_session(&self.events, &session, animation, None);
        }
        self.controller.poke();
        Ok(())
    }

    pub fn get_state(&self) -> SessionView {
        self.lock().view()
    }

    pub fn new_game(&self) -> Result<(), ApiError> {
        self.mutate(|s| {
            s.new_game();
            Ok(Vec::new())
        })
    }

    pub fn load_game(&self, record: &str) -> Result<(), ApiError> {
        self.mutate(|s| s.load(record).map(|_| Vec::new()))
    }

    pub fn export_game(&self, main_line_only: bool) -> String {
        self.lock().export(main_line_only)
    }

    pub fn goto_ply(&self, ply: usize) -> Result<(), ApiError> {
        self.mutate(|s| s.goto(ply))
    }

    pub fn goto_node(&self, node: NodeId) -> Result<(), ApiError> {
        self.mutate(|s| s.goto_node(node))
    }

    pub fn goto_sibling(&self, offset: isize) -> Result<(), ApiError> {
        self.mutate(|s| s.goto_sibling(offset))
    }

    pub fn goto_branch(&self, forward: bool) -> Result<(), ApiError> {
        self.mutate(|s| s.goto_branch(forward))
    }

    pub fn set_comment(&self, node: NodeId, text: &str) -> Result<(), ApiError> {
        self.mutate(|s| s.set_comment(node, text))
    }

    pub fn toggle_glyph(&self, node: NodeId, glyph: Glyph) -> Result<(), ApiError> {
        self.mutate(|s| s.toggle_glyph(node, glyph))
    }

    pub fn toggle_collapsed(&self, node: NodeId) -> Result<(), ApiError> {
        self.mutate(|s| s.toggle_collapsed(node))
    }

    pub fn promote(&self, node: NodeId) -> Result<(), ApiError> {
        self.mutate(|s| s.promote(node))
    }

    pub fn demote(&self, node: NodeId) -> Result<(), ApiError> {
        self.mutate(|s| s.demote(node))
    }

    pub fn make_main_line(&self, node: NodeId) -> Result<(), ApiError> {
        self.mutate(|s| s.make_main_line(node))
    }

    pub fn delete_from(&self, node: NodeId) -> Result<(), ApiError> {
        self.mutate(|s| s.delete_from(node))
    }

    pub fn goto_live(&self) -> Result<(), ApiError> {
        self.mutate(|s| {
            s.goto_live();
            Ok(Vec::new())
        })
    }

    pub fn move_replay(&self) -> Option<MoveReplay> {
        self.lock().move_replay()
    }

    pub fn legal_targets(&self, from: Square) -> Vec<StepTarget> {
        self.lock().legal_targets(from)
    }

    pub fn try_step(&self, from: Square, to: Square) -> Result<(), ApiError> {
        self.mutate(|s| s.try_step(from, to))
    }

    pub fn plan_route(&self, from: Square, to: Square, path: &[Square]) -> Option<Vec<Square>> {
        self.lock().plan_route(from, to, path)
    }

    pub fn try_route(&self, from: Square, to: Square, path: &[Square]) -> Result<(), ApiError> {
        self.mutate(|s| s.try_route(from, to, path))
    }

    pub fn undo_step(&self) -> Result<(), ApiError> {
        self.mutate(|s| s.undo_step())
    }

    pub fn cancel_turn(&self) -> Result<(), ApiError> {
        self.mutate(|s| Ok(s.cancel_turn()))
    }

    /// Ends the turn being entered: plays it in a match on the user's move
    /// (or plays the shown plan's move), unless `plan` keeps it as a plan.
    pub fn commit_turn(&self, plan: bool) -> Result<(), ApiError> {
        self.mutate(|s| s.commit_turn(plan).map(|_| Vec::new()))
    }

    /// Takes back played moves to the last human move (one ply between engines).
    pub fn take_back(&self) -> Result<(), ApiError> {
        self.mutate(|s| s.take_back())
    }

    /// Whether a step after a full turn starts the other side's turn.
    pub fn set_continue_turns(&self, on: bool) -> Result<(), ApiError> {
        self.mutate(|s| {
            s.set_continue_turns(on);
            Ok(Vec::new())
        })
    }

    pub fn setup_swap(&self, a: Square, b: Square) -> Result<(), ApiError> {
        self.mutate(|s| s.setup_swap(a, b))
    }

    pub fn commit_setup(&self) -> Result<(), ApiError> {
        self.mutate(|s| s.commit_setup().map(|_| Vec::new()))
    }

    pub fn start_match(&self, spec: &MatchSpec) -> Result<(), ApiError> {
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
        self.mutate(|s| {
            s.start_match(players, tcs, spec.takebacks);
            Ok(Vec::new())
        })
    }

    pub fn end_match(&self) -> Result<(), ApiError> {
        self.mutate(|s| {
            s.end_match();
            Ok(Vec::new())
        })
    }

    /// Turns analysis on with the engine `engine_id`, or off with `None`.
    /// (It will be refused while the user plays an online game.)
    pub fn set_analysis(&self, engine_id: Option<&str>) -> Result<(), ApiError> {
        let engine = match engine_id {
            Some(id) => {
                let spec = self
                    .engines()
                    .get(id)
                    .ok_or_else(|| ApiError::illegal(format!("unknown engine {id:?}")))?;
                Some(AnalysisEngine { id: spec.id, name: spec.name })
            }
            None => None,
        };
        self.mutate(|s| {
            s.set_analysis(engine);
            Ok(Vec::new())
        })
    }

    /// Adds moves as a line from `from` and shows its end (see
    /// [`Session::add_line`]).
    pub fn add_line(&self, from: NodeId, moves: &[String]) -> Result<(), ApiError> {
        self.mutate(|s| s.add_line(from, moves))
    }

    pub fn preview_line(&self, from: NodeId, moves: &[String]) -> Result<PositionView, ApiError> {
        self.lock().preview_line(from, moves)
    }

    pub fn engine_move_now(&self) {
        self.controller.move_now();
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

    /// Runs a command by name with JSON arguments, as `invoke(cmd, args)`
    /// does in the app. Used by the dev bridge. Keep it in step with the
    /// handler list in `lib.rs`; a test checks it covers `api.ts`.
    pub async fn dispatch(&self, cmd: &str, args: &Value) -> Result<Value, ApiError> {
        fn ok(v: impl Serialize) -> Result<Value, ApiError> {
            serde_json::to_value(v)
                .map_err(|e| ApiError::state(format!("couldn't serialize the result: {e}")))
        }
        match cmd {
            "get_state" => ok(self.get_state()),
            "new_game" => ok(self.new_game()?),
            "load_game" => ok(self.load_game(&arg::<String>(args, "record")?)?),
            "export_game" => ok(self.export_game(arg(args, "mainLineOnly")?)),
            "goto_ply" => ok(self.goto_ply(arg(args, "ply")?)?),
            "goto_live" => ok(self.goto_live()?),
            "goto_node" => ok(self.goto_node(arg(args, "node")?)?),
            "goto_sibling" => ok(self.goto_sibling(arg(args, "offset")?)?),
            "goto_branch" => ok(self.goto_branch(arg(args, "forward")?)?),
            "set_comment" => ok(self.set_comment(arg(args, "node")?, &arg::<String>(args, "text")?)?),
            "toggle_glyph" => ok(self.toggle_glyph(arg(args, "node")?, arg(args, "glyph")?)?),
            "toggle_collapsed" => ok(self.toggle_collapsed(arg(args, "node")?)?),
            "promote" => ok(self.promote(arg(args, "node")?)?),
            "demote" => ok(self.demote(arg(args, "node")?)?),
            "make_main_line" => ok(self.make_main_line(arg(args, "node")?)?),
            "delete_from" => ok(self.delete_from(arg(args, "node")?)?),
            "move_replay" => ok(self.move_replay()),
            "legal_targets" => ok(self.legal_targets(arg(args, "from")?)),
            "try_step" => ok(self.try_step(arg(args, "from")?, arg(args, "to")?)?),
            "plan_route" => {
                ok(self.plan_route(arg(args, "from")?, arg(args, "to")?, &arg::<Vec<Square>>(args, "path")?))
            }
            "try_route" => ok(self.try_route(
                arg(args, "from")?,
                arg(args, "to")?,
                &arg::<Vec<Square>>(args, "path")?,
            )?),
            "undo_step" => ok(self.undo_step()?),
            "cancel_turn" => ok(self.cancel_turn()?),
            "commit_turn" => ok(self.commit_turn(arg(args, "plan")?)?),
            "take_back" => ok(self.take_back()?),
            "set_continue_turns" => ok(self.set_continue_turns(arg(args, "on")?)?),
            "setup_swap" => ok(self.setup_swap(arg(args, "a")?, arg(args, "b")?)?),
            "commit_setup" => ok(self.commit_setup()?),
            "start_match" => ok(self.start_match(&arg(args, "spec")?)?),
            "end_match" => ok(self.end_match()?),
            "engine_move_now" => {
                self.engine_move_now();
                ok(())
            }
            "set_analysis" => ok(self.set_analysis(arg::<Option<String>>(args, "engineId")?.as_deref())?),
            "add_line" => ok(self.add_line(arg(args, "from")?, &arg::<Vec<String>>(args, "moves")?)?),
            "preview_line" => ok(self.preview_line(arg(args, "from")?, &arg::<Vec<String>>(args, "moves")?)?),
            "list_engines" => ok(self.list_engines()),
            "save_engine" => ok(self.save_engine(arg(args, "spec")?)?),
            "delete_engine" => ok(self.delete_engine(&arg::<String>(args, "id")?)?),
            "test_engine" => ok(Backend::test_engine(&arg(args, "spec")?).await?),
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

    /// Every command `api.ts` invokes must be handled by `dispatch`, or the
    /// dev bridge silently falls behind the app.
    #[tokio::test]
    async fn dispatch_covers_api_ts() {
        let rt = tokio::runtime::Handle::current();
        tauri::async_runtime::set(rt);
        let path = std::env::temp_dir().join(format!("arimaa-dispatch-test-{}.json", std::process::id()));
        let backend = Backend::new(EngineRegistry::load(path), Arc::new(NullSink));
        let api = include_str!("../../src/lib/api.ts");
        let names: Vec<&str> = api.split("invoke<").skip(1).filter_map(|s| s.split('\'').nth(1)).collect();
        assert!(names.len() > 10, "found only {names:?} in api.ts");
        for name in names {
            if let Err(e) = backend.dispatch(name, &Value::Null).await {
                assert!(!e.message.starts_with("unknown command"), "dispatch doesn't handle {name}");
            }
        }
    }
}
