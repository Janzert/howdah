//! Tauri commands: thin wrappers over [`Backend`], which holds the logic
//! and documents the conventions.

use howdah_arimaa::{Glyph, NodeId, Square};
use tauri::State;

use crate::backend::Backend;
use crate::dto::{
    ApiError, EngineIdentity, EngineSpec, GameroomStatus, LiveGameView, MatchSpec, MoveReplay, PositionView,
    SessionId, SessionView, StepTarget, WatchView,
};

#[tauri::command]
pub fn open_session(state: State<Backend>) -> SessionId {
    state.open_session()
}

#[tauri::command]
pub fn close_session(state: State<Backend>, session: SessionId) -> Result<(), ApiError> {
    state.close_session(session)
}

#[tauri::command]
pub fn list_sessions(state: State<Backend>) -> Vec<SessionId> {
    state.list_sessions()
}

#[tauri::command]
pub fn get_state(state: State<Backend>, session: SessionId) -> Result<SessionView, ApiError> {
    state.get_state(session)
}

#[tauri::command]
pub fn new_game(state: State<Backend>, session: SessionId) -> Result<(), ApiError> {
    state.new_game(session)
}

#[tauri::command]
pub fn load_game(state: State<Backend>, session: SessionId, record: String) -> Result<(), ApiError> {
    state.load_game(session, &record)
}

#[tauri::command]
pub fn export_game(
    state: State<Backend>,
    session: SessionId,
    main_line_only: bool,
) -> Result<String, ApiError> {
    state.export_game(session, main_line_only)
}

#[tauri::command]
pub fn goto_ply(state: State<Backend>, session: SessionId, ply: usize) -> Result<(), ApiError> {
    state.goto_ply(session, ply)
}

#[tauri::command]
pub fn goto_node(state: State<Backend>, session: SessionId, node: NodeId) -> Result<(), ApiError> {
    state.goto_node(session, node)
}

#[tauri::command]
pub fn goto_sibling(state: State<Backend>, session: SessionId, offset: isize) -> Result<(), ApiError> {
    state.goto_sibling(session, offset)
}

#[tauri::command]
pub fn goto_branch(state: State<Backend>, session: SessionId, forward: bool) -> Result<(), ApiError> {
    state.goto_branch(session, forward)
}

#[tauri::command]
pub fn set_comment(
    state: State<Backend>,
    session: SessionId,
    node: NodeId,
    text: String,
) -> Result<(), ApiError> {
    state.set_comment(session, node, &text)
}

#[tauri::command]
pub fn toggle_glyph(
    state: State<Backend>,
    session: SessionId,
    node: NodeId,
    glyph: Glyph,
) -> Result<(), ApiError> {
    state.toggle_glyph(session, node, glyph)
}

#[tauri::command]
pub fn toggle_collapsed(state: State<Backend>, session: SessionId, node: NodeId) -> Result<(), ApiError> {
    state.toggle_collapsed(session, node)
}

#[tauri::command]
pub fn promote(state: State<Backend>, session: SessionId, node: NodeId) -> Result<(), ApiError> {
    state.promote(session, node)
}

#[tauri::command]
pub fn demote(state: State<Backend>, session: SessionId, node: NodeId) -> Result<(), ApiError> {
    state.demote(session, node)
}

#[tauri::command]
pub fn make_main_line(state: State<Backend>, session: SessionId, node: NodeId) -> Result<(), ApiError> {
    state.make_main_line(session, node)
}

#[tauri::command]
pub fn delete_from(state: State<Backend>, session: SessionId, node: NodeId) -> Result<(), ApiError> {
    state.delete_from(session, node)
}

#[tauri::command]
pub fn goto_live(state: State<Backend>, session: SessionId) -> Result<(), ApiError> {
    state.goto_live(session)
}

#[tauri::command]
pub fn move_replay(state: State<Backend>, session: SessionId) -> Result<Option<MoveReplay>, ApiError> {
    state.move_replay(session)
}

#[tauri::command]
pub fn legal_targets(
    state: State<Backend>,
    session: SessionId,
    from: Square,
) -> Result<Vec<StepTarget>, ApiError> {
    state.legal_targets(session, from)
}

#[tauri::command]
pub fn try_step(state: State<Backend>, session: SessionId, from: Square, to: Square) -> Result<(), ApiError> {
    state.try_step(session, from, to)
}

#[tauri::command]
pub fn plan_route(
    state: State<Backend>,
    session: SessionId,
    from: Square,
    to: Square,
    path: Vec<Square>,
) -> Result<Option<Vec<Square>>, ApiError> {
    state.plan_route(session, from, to, &path)
}

#[tauri::command]
pub fn try_route(
    state: State<Backend>,
    session: SessionId,
    from: Square,
    to: Square,
    path: Vec<Square>,
) -> Result<(), ApiError> {
    state.try_route(session, from, to, &path)
}

#[tauri::command]
pub fn undo_step(state: State<Backend>, session: SessionId) -> Result<(), ApiError> {
    state.undo_step(session)
}

#[tauri::command]
pub fn cancel_turn(state: State<Backend>, session: SessionId) -> Result<(), ApiError> {
    state.cancel_turn(session)
}

#[tauri::command]
pub fn commit_turn(state: State<Backend>, session: SessionId, plan: bool) -> Result<(), ApiError> {
    state.commit_turn(session, plan)
}

#[tauri::command]
pub fn take_back(state: State<Backend>, session: SessionId) -> Result<(), ApiError> {
    state.take_back(session)
}

#[tauri::command]
pub fn set_continue_turns(state: State<Backend>, session: SessionId, on: bool) -> Result<(), ApiError> {
    state.set_continue_turns(session, on)
}

#[tauri::command]
pub fn setup_swap(state: State<Backend>, session: SessionId, a: Square, b: Square) -> Result<(), ApiError> {
    state.setup_swap(session, a, b)
}

#[tauri::command]
pub fn commit_setup(state: State<Backend>, session: SessionId) -> Result<(), ApiError> {
    state.commit_setup(session)
}

#[tauri::command]
pub fn start_match(state: State<Backend>, session: SessionId, spec: MatchSpec) -> Result<(), ApiError> {
    state.start_match(session, &spec)
}

#[tauri::command]
pub fn end_match(state: State<Backend>, session: SessionId) -> Result<(), ApiError> {
    state.end_match(session)
}

#[tauri::command]
pub fn engine_move_now(state: State<Backend>, session: SessionId) -> Result<(), ApiError> {
    state.engine_move_now(session)
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
pub fn set_analysis(
    state: State<Backend>,
    session: SessionId,
    engine_id: Option<String>,
) -> Result<(), ApiError> {
    state.set_analysis(session, engine_id.as_deref())
}

#[tauri::command]
pub fn add_line(
    state: State<Backend>,
    session: SessionId,
    from: NodeId,
    moves: Vec<String>,
) -> Result<(), ApiError> {
    state.add_line(session, from, &moves)
}

#[tauri::command]
pub fn preview_line(
    state: State<Backend>,
    session: SessionId,
    from: NodeId,
    moves: Vec<String>,
) -> Result<PositionView, ApiError> {
    state.preview_line(session, from, &moves)
}

#[tauri::command]
pub fn gameroom_status(state: State<Backend>) -> GameroomStatus {
    state.gameroom_status()
}

#[tauri::command]
pub async fn gameroom_login(
    state: State<'_, Backend>,
    username: String,
    password: String,
    remember: bool,
) -> Result<GameroomStatus, ApiError> {
    state.gameroom_login(&username, &password, remember).await
}

#[tauri::command]
pub async fn gameroom_logout(state: State<'_, Backend>) -> Result<(), ApiError> {
    state.gameroom_logout().await
}

#[tauri::command]
pub async fn live_games(state: State<'_, Backend>) -> Result<Vec<LiveGameView>, ApiError> {
    state.live_games().await
}

#[tauri::command]
pub async fn watch_game(state: State<'_, Backend>, session: SessionId, gid: String) -> Result<(), ApiError> {
    state.watch_game(session, &gid).await
}

#[tauri::command]
pub fn stop_watching(state: State<Backend>, session: SessionId) -> Result<(), ApiError> {
    state.stop_watching(session)
}

#[tauri::command]
pub fn watch_status(state: State<Backend>, session: SessionId) -> Result<Option<WatchView>, ApiError> {
    state.watch_status(session)
}
