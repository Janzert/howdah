//! Tauri commands. Commands are intents: mutating commands return only
//! success or an error, and the new state arrives as a `game://changed`
//! event. Queries (`get_state`, `legal_targets`, `export_game`, the engine
//! list) return data.
//!
//! Every state source (UI intents, the engine controller, later the
//! gameroom) goes through the same event path, so the frontend has one place
//! where state comes in.

use std::sync::MutexGuard;
use std::time::Duration;

use arimaa_core::{Square, TimeControl};
use tauri::{AppHandle, Emitter, State};

use crate::controller::{Controller, SharedRegistry, SharedSession};
use crate::dto::{
    AnimStep, ApiError, EngineIdentity, EngineSpec, MatchSpec, PlayerSpec, SessionUpdate, SessionView,
    StepTarget,
};
use crate::engines;
use crate::session::{Player, Session};

/// Event carrying a [`SessionUpdate`] after every change to the session.
pub const GAME_CHANGED: &str = "game://changed";

pub struct AppState {
    pub session: SharedSession,
    pub engines: SharedRegistry,
    pub controller: Controller,
}

impl AppState {
    fn lock(&self) -> MutexGuard<'_, Session> {
        // A panic mid-command can't leave the session half-updated in a way
        // that matters more than refusing all further commands would.
        self.session.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn engines(&self) -> MutexGuard<'_, engines::EngineRegistry> {
        self.engines.lock().unwrap_or_else(|e| e.into_inner())
    }
}

pub fn emit_session(app: &AppHandle, session: &Session, animation: Vec<AnimStep>, budget: Option<Duration>) {
    let update = SessionUpdate {
        view: session.view(),
        animation,
        animation_budget_ms: budget.map(|d| d.as_millis() as u64),
    };
    if let Err(e) = app.emit(GAME_CHANGED, update) {
        eprintln!("failed to emit {GAME_CHANGED}: {e}");
    }
}

/// Runs a mutation, broadcasts the result, and lets the controller react.
fn mutate(
    app: &AppHandle,
    state: &State<AppState>,
    f: impl FnOnce(&mut Session) -> Result<Vec<AnimStep>, ApiError>,
) -> Result<(), ApiError> {
    {
        let mut session = state.lock();
        let animation = f(&mut session)?;
        emit_session(app, &session, animation, None);
    }
    state.controller.poke();
    Ok(())
}

#[tauri::command]
pub fn get_state(state: State<AppState>) -> SessionView {
    state.lock().view()
}

#[tauri::command]
pub fn new_game(app: AppHandle, state: State<AppState>) -> Result<(), ApiError> {
    mutate(&app, &state, |s| {
        s.new_game();
        Ok(Vec::new())
    })
}

#[tauri::command]
pub fn load_game(app: AppHandle, state: State<AppState>, record: String) -> Result<(), ApiError> {
    mutate(&app, &state, |s| s.load(&record).map(|_| Vec::new()))
}

#[tauri::command]
pub fn export_game(state: State<AppState>) -> String {
    state.lock().export()
}

#[tauri::command]
pub fn goto_ply(app: AppHandle, state: State<AppState>, ply: usize) -> Result<(), ApiError> {
    mutate(&app, &state, |s| s.goto(ply))
}

#[tauri::command]
pub fn legal_targets(state: State<AppState>, from: Square) -> Vec<StepTarget> {
    state.lock().legal_targets(from)
}

#[tauri::command]
pub fn try_step(app: AppHandle, state: State<AppState>, from: Square, to: Square) -> Result<(), ApiError> {
    mutate(&app, &state, |s| s.try_step(from, to))
}

#[tauri::command]
pub fn plan_route(
    state: State<AppState>,
    from: Square,
    to: Square,
    path: Vec<Square>,
) -> Option<Vec<Square>> {
    state.lock().plan_route(from, to, &path)
}

#[tauri::command]
pub fn try_route(
    app: AppHandle,
    state: State<AppState>,
    from: Square,
    to: Square,
    path: Vec<Square>,
) -> Result<(), ApiError> {
    mutate(&app, &state, |s| s.try_route(from, to, &path))
}

#[tauri::command]
pub fn undo_step(app: AppHandle, state: State<AppState>) -> Result<(), ApiError> {
    mutate(&app, &state, |s| s.undo_step())
}

#[tauri::command]
pub fn cancel_turn(app: AppHandle, state: State<AppState>) -> Result<(), ApiError> {
    mutate(&app, &state, |s| Ok(s.cancel_turn()))
}

#[tauri::command]
pub fn commit_turn(app: AppHandle, state: State<AppState>) -> Result<(), ApiError> {
    mutate(&app, &state, |s| s.commit_turn().map(|_| Vec::new()))
}

#[tauri::command]
pub fn setup_swap(app: AppHandle, state: State<AppState>, a: Square, b: Square) -> Result<(), ApiError> {
    mutate(&app, &state, |s| s.setup_swap(a, b))
}

#[tauri::command]
pub fn commit_setup(app: AppHandle, state: State<AppState>) -> Result<(), ApiError> {
    mutate(&app, &state, |s| s.commit_setup().map(|_| Vec::new()))
}

#[tauri::command]
pub fn start_match(app: AppHandle, state: State<AppState>, spec: MatchSpec) -> Result<(), ApiError> {
    let player = |p: &PlayerSpec| -> Result<Player, ApiError> {
        match p {
            PlayerSpec::Human => Ok(Player::Human),
            PlayerSpec::Engine { engine_id } => {
                let e = state
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
            Some(t) => t.parse().map(Some).map_err(|e| ApiError::illegal(format!("time control {t:?}: {e}"))),
            None => Ok(None),
        }
    };
    let tcs = [parse(&spec.gold_time_control)?, parse(&spec.silver_time_control)?];
    mutate(&app, &state, |s| {
        s.start_match(players, tcs);
        Ok(Vec::new())
    })
}

#[tauri::command]
pub fn end_match(app: AppHandle, state: State<AppState>) -> Result<(), ApiError> {
    mutate(&app, &state, |s| {
        s.end_match();
        Ok(Vec::new())
    })
}

#[tauri::command]
pub fn engine_move_now(state: State<AppState>) {
    state.controller.move_now();
}

#[tauri::command]
pub fn list_engines(state: State<AppState>) -> Vec<EngineSpec> {
    state.engines().list()
}

#[tauri::command]
pub fn save_engine(state: State<AppState>, spec: EngineSpec) -> Result<EngineSpec, ApiError> {
    state.engines().save(spec)
}

#[tauri::command]
pub fn delete_engine(state: State<AppState>, id: String) -> Result<(), ApiError> {
    state.engines().delete(&id)
}

/// Starts the engine to check it speaks AEI, and reports its identity.
#[tauri::command]
pub async fn test_engine(spec: EngineSpec) -> Result<EngineIdentity, ApiError> {
    engines::probe(&spec).await
}
