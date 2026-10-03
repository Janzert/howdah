//! Tauri commands: thin wrappers over [`Backend`], which holds the logic
//! and documents the conventions.

use arimaa_core::Square;
use tauri::State;

use crate::backend::Backend;
use crate::dto::{ApiError, EngineIdentity, EngineSpec, MatchSpec, MoveReplay, SessionView, StepTarget};

#[tauri::command]
pub fn get_state(state: State<Backend>) -> SessionView {
    state.get_state()
}

#[tauri::command]
pub fn new_game(state: State<Backend>) -> Result<(), ApiError> {
    state.new_game()
}

#[tauri::command]
pub fn load_game(state: State<Backend>, record: String) -> Result<(), ApiError> {
    state.load_game(&record)
}

#[tauri::command]
pub fn export_game(state: State<Backend>) -> String {
    state.export_game()
}

#[tauri::command]
pub fn goto_ply(state: State<Backend>, ply: usize) -> Result<(), ApiError> {
    state.goto_ply(ply)
}

#[tauri::command]
pub fn goto_live(state: State<Backend>) -> Result<(), ApiError> {
    state.goto_live()
}

#[tauri::command]
pub fn move_replay(state: State<Backend>) -> Option<MoveReplay> {
    state.move_replay()
}

#[tauri::command]
pub fn legal_targets(state: State<Backend>, from: Square) -> Vec<StepTarget> {
    state.legal_targets(from)
}

#[tauri::command]
pub fn try_step(state: State<Backend>, from: Square, to: Square) -> Result<(), ApiError> {
    state.try_step(from, to)
}

#[tauri::command]
pub fn plan_route(state: State<Backend>, from: Square, to: Square, path: Vec<Square>) -> Option<Vec<Square>> {
    state.plan_route(from, to, &path)
}

#[tauri::command]
pub fn try_route(state: State<Backend>, from: Square, to: Square, path: Vec<Square>) -> Result<(), ApiError> {
    state.try_route(from, to, &path)
}

#[tauri::command]
pub fn undo_step(state: State<Backend>) -> Result<(), ApiError> {
    state.undo_step()
}

#[tauri::command]
pub fn cancel_turn(state: State<Backend>) -> Result<(), ApiError> {
    state.cancel_turn()
}

#[tauri::command]
pub fn commit_turn(state: State<Backend>) -> Result<(), ApiError> {
    state.commit_turn()
}

#[tauri::command]
pub fn setup_swap(state: State<Backend>, a: Square, b: Square) -> Result<(), ApiError> {
    state.setup_swap(a, b)
}

#[tauri::command]
pub fn commit_setup(state: State<Backend>) -> Result<(), ApiError> {
    state.commit_setup()
}

#[tauri::command]
pub fn start_match(state: State<Backend>, spec: MatchSpec) -> Result<(), ApiError> {
    state.start_match(&spec)
}

#[tauri::command]
pub fn end_match(state: State<Backend>) -> Result<(), ApiError> {
    state.end_match()
}

#[tauri::command]
pub fn engine_move_now(state: State<Backend>) {
    state.engine_move_now();
}

#[tauri::command]
pub fn list_engines(state: State<Backend>) -> Vec<EngineSpec> {
    state.list_engines()
}

#[tauri::command]
pub fn save_engine(state: State<Backend>, spec: EngineSpec) -> Result<EngineSpec, ApiError> {
    state.save_engine(spec)
}

#[tauri::command]
pub fn delete_engine(state: State<Backend>, id: String) -> Result<(), ApiError> {
    state.delete_engine(&id)
}

#[tauri::command]
pub async fn test_engine(spec: EngineSpec) -> Result<EngineIdentity, ApiError> {
    Backend::test_engine(&spec).await
}
