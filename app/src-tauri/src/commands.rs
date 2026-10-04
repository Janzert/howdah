//! Tauri commands: thin wrappers over [`Backend`], which holds the logic
//! and documents the conventions.

use arimaa_core::{Glyph, NodeId, Square};
use tauri::State;

use crate::backend::Backend;
use crate::dto::{
    ApiError, EngineIdentity, EngineSpec, MatchSpec, MoveReplay, PositionView, SessionView, StepTarget,
};

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
pub fn export_game(state: State<Backend>, main_line_only: bool) -> String {
    state.export_game(main_line_only)
}

#[tauri::command]
pub fn goto_ply(state: State<Backend>, ply: usize) -> Result<(), ApiError> {
    state.goto_ply(ply)
}

#[tauri::command]
pub fn goto_node(state: State<Backend>, node: NodeId) -> Result<(), ApiError> {
    state.goto_node(node)
}

#[tauri::command]
pub fn goto_sibling(state: State<Backend>, offset: isize) -> Result<(), ApiError> {
    state.goto_sibling(offset)
}

#[tauri::command]
pub fn goto_branch(state: State<Backend>, forward: bool) -> Result<(), ApiError> {
    state.goto_branch(forward)
}

#[tauri::command]
pub fn set_comment(state: State<Backend>, node: NodeId, text: String) -> Result<(), ApiError> {
    state.set_comment(node, &text)
}

#[tauri::command]
pub fn toggle_glyph(state: State<Backend>, node: NodeId, glyph: Glyph) -> Result<(), ApiError> {
    state.toggle_glyph(node, glyph)
}

#[tauri::command]
pub fn toggle_collapsed(state: State<Backend>, node: NodeId) -> Result<(), ApiError> {
    state.toggle_collapsed(node)
}

#[tauri::command]
pub fn promote(state: State<Backend>, node: NodeId) -> Result<(), ApiError> {
    state.promote(node)
}

#[tauri::command]
pub fn demote(state: State<Backend>, node: NodeId) -> Result<(), ApiError> {
    state.demote(node)
}

#[tauri::command]
pub fn make_main_line(state: State<Backend>, node: NodeId) -> Result<(), ApiError> {
    state.make_main_line(node)
}

#[tauri::command]
pub fn delete_from(state: State<Backend>, node: NodeId) -> Result<(), ApiError> {
    state.delete_from(node)
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
pub fn commit_turn(state: State<Backend>, plan: bool) -> Result<(), ApiError> {
    state.commit_turn(plan)
}

#[tauri::command]
pub fn take_back(state: State<Backend>) -> Result<(), ApiError> {
    state.take_back()
}

#[tauri::command]
pub fn set_continue_turns(state: State<Backend>, on: bool) -> Result<(), ApiError> {
    state.set_continue_turns(on)
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

#[tauri::command]
pub fn set_analysis(state: State<Backend>, engine_id: Option<String>) -> Result<(), ApiError> {
    state.set_analysis(engine_id.as_deref())
}

#[tauri::command]
pub fn add_line(state: State<Backend>, from: NodeId, moves: Vec<String>) -> Result<(), ApiError> {
    state.add_line(from, &moves)
}

#[tauri::command]
pub fn preview_line(
    state: State<Backend>,
    from: NodeId,
    moves: Vec<String>,
) -> Result<PositionView, ApiError> {
    state.preview_line(from, &moves)
}
