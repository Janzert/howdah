//! Tauri commands. Commands are intents: mutating commands return only
//! success or an error, and the new state arrives as a `game://changed`
//! event. Queries (`get_state`, `legal_targets`, `export_game`) return data.
//!
//! Every state source (UI intents now; engines, the gameroom and tournaments
//! later) goes through the same event path, so the frontend has one place
//! where state comes in.

use std::sync::{Mutex, MutexGuard};

use arimaa_core::Square;
use tauri::{AppHandle, Emitter, State};

use crate::dto::{AnimStep, ApiError, SessionUpdate, SessionView, StepTarget};
use crate::session::Session;

/// Event carrying a [`SessionUpdate`] after every change to the session.
pub const GAME_CHANGED: &str = "game://changed";

#[derive(Default)]
pub struct AppState {
    session: Mutex<Session>,
}

impl AppState {
    fn lock(&self) -> MutexGuard<'_, Session> {
        // A panic mid-command can't leave the session half-updated in a way
        // that matters more than refusing all further commands would.
        self.session.lock().unwrap_or_else(|e| e.into_inner())
    }
}

fn emit(app: &AppHandle, session: &Session, animation: Vec<AnimStep>) {
    let update = SessionUpdate { view: session.view(), animation };
    if let Err(e) = app.emit(GAME_CHANGED, update) {
        eprintln!("failed to emit {GAME_CHANGED}: {e}");
    }
}

/// Runs a mutation and broadcasts the result.
fn mutate(
    app: &AppHandle,
    state: &State<AppState>,
    f: impl FnOnce(&mut Session) -> Result<Vec<AnimStep>, ApiError>,
) -> Result<(), ApiError> {
    let mut session = state.lock();
    let animation = f(&mut session)?;
    emit(app, &session, animation);
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
