//! Tauri commands: thin wrappers over [`Backend`], which holds the logic
//! and documents the conventions.

use howdah_arimaa::{Color, Glyph, NodeId, Square};
use tauri::State;

use crate::backend::{self, Backend};
use crate::dto::{
    ApiError, ArchivedTestView, BotInfoView, ChatArchiveDay, ChatArchiveMonth, ChatMessageView, ChatRoomView,
    EngineCatalogView, EngineIdentity, EngineOption, EngineOutput, EngineSpec, GameroomGames, GameroomStatus,
    MatchSpec, MoveReplay, NewTestSpec, PlayerGamesView, PlayerMatchView, PositionCheck, PositionSpec,
    PositionView, PostalGameView, PuzzleGroupView, ServerBotsView, SessionId, SessionView, StartedBotView,
    StepTarget, TestDetailView, TestView, TourneyStatusView, WatchView,
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
pub fn docked_sessions(state: State<Backend>) -> Vec<SessionId> {
    state.docked_sessions()
}

#[tauri::command]
pub fn set_session_docked(state: State<Backend>, session: SessionId, docked: bool) -> Result<(), ApiError> {
    state.set_session_docked(session, docked)
}

#[tauri::command]
pub fn show_board(state: State<Backend>, session: SessionId) -> Result<(), ApiError> {
    state.show_board(session)
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
pub fn new_game_from(
    state: State<Backend>,
    session: SessionId,
    position: PositionSpec,
) -> Result<(), ApiError> {
    state.new_game_from(session, &position)
}

#[tauri::command]
pub fn editor_position(state: State<Backend>, session: SessionId) -> Result<PositionSpec, ApiError> {
    state.editor_position(session)
}

#[tauri::command]
pub fn parse_position(text: String) -> Result<PositionSpec, ApiError> {
    backend::parse_position(&text)
}

#[tauri::command]
pub fn check_position(position: PositionSpec) -> Result<PositionCheck, ApiError> {
    backend::check_position(&position)
}

#[tauri::command]
pub fn edit_position(position: PositionSpec, text: String) -> Result<PositionSpec, ApiError> {
    backend::edit_position(&position, &text)
}

#[tauri::command]
pub fn default_position() -> PositionSpec {
    backend::default_position()
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
pub fn request_takeback(state: State<Backend>, session: SessionId) -> Result<(), ApiError> {
    state.request_takeback(session)
}

#[tauri::command]
pub fn answer_takeback(state: State<Backend>, session: SessionId, accept: bool) -> Result<(), ApiError> {
    state.answer_takeback(session, accept)
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
pub fn set_engine_options(
    state: State<Backend>,
    session: SessionId,
    side: Color,
    options: Vec<EngineOption>,
) -> Result<(), ApiError> {
    state.set_engine_options(session, side, options)
}

#[tauri::command]
pub fn set_analysis_options(
    state: State<Backend>,
    session: SessionId,
    options: Vec<EngineOption>,
) -> Result<(), ApiError> {
    state.set_analysis_options(session, options)
}

#[tauri::command]
pub fn press_engine_button(
    state: State<Backend>,
    session: SessionId,
    side: Option<Color>,
    name: String,
) -> Result<(), ApiError> {
    state.press_engine_button(session, side, name)
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
pub fn engine_catalog(state: State<Backend>) -> EngineCatalogView {
    state.engine_catalog()
}

#[tauri::command]
pub async fn add_engine_manifest(
    state: State<'_, Backend>,
    url: Option<String>,
    text: Option<String>,
) -> Result<EngineCatalogView, ApiError> {
    state.add_engine_manifest(url.as_deref(), text.as_deref()).await
}

#[tauri::command]
pub fn remove_engine_manifest(state: State<Backend>, id: String) -> Result<EngineCatalogView, ApiError> {
    state.remove_engine_manifest(&id)
}

#[tauri::command]
pub async fn refresh_engine_manifest(
    state: State<'_, Backend>,
    id: String,
) -> Result<EngineCatalogView, ApiError> {
    state.refresh_engine_manifest(&id).await
}

#[tauri::command]
pub async fn install_engine(state: State<'_, Backend>, id: String) -> Result<EngineSpec, ApiError> {
    state.install_engine(&id).await
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
pub async fn gameroom_games(state: State<'_, Backend>) -> Result<GameroomGames, ApiError> {
    state.gameroom_games().await
}

#[tauri::command]
pub fn gameroom_game_session(state: State<Backend>, gid: String) -> Option<SessionId> {
    state.gameroom_game_session(&gid)
}

#[tauri::command]
pub fn open_url(state: State<Backend>, url: String) -> Result<(), ApiError> {
    state.open_url(&url)
}

#[tauri::command]
pub fn chat_archive_month(
    state: State<Backend>,
    year: i32,
    month: u32,
) -> Result<ChatArchiveMonth, ApiError> {
    state.chat_archive_month(year, month)
}

#[tauri::command]
pub async fn chat_archive_day(
    state: State<'_, Backend>,
    date: String,
    fetch: bool,
) -> Result<ChatArchiveDay, ApiError> {
    state.chat_archive_day(&date, fetch).await
}

#[tauri::command]
pub fn chatroom_state(state: State<Backend>) -> ChatRoomView {
    state.chatroom_state()
}

#[tauri::command]
pub async fn enter_chatroom(state: State<'_, Backend>, auto_leave: bool) -> Result<ChatRoomView, ApiError> {
    state.enter_chatroom(auto_leave).await
}

#[tauri::command]
pub async fn leave_chatroom(state: State<'_, Backend>) -> Result<(), ApiError> {
    state.leave_chatroom().await
}

#[tauri::command]
pub fn format_chat_message(state: State<Backend>, text: String) -> ChatMessageView {
    state.format_chat_message(&text)
}

#[tauri::command]
pub async fn say_in_chatroom(state: State<'_, Backend>, text: String, color: String) -> Result<(), ApiError> {
    state.say_in_chatroom(&text, &color).await
}

#[tauri::command]
pub fn set_chatroom_active(state: State<Backend>, active: bool) {
    state.set_chatroom_active(active)
}

#[tauri::command]
pub fn set_chatroom_auto_leave(state: State<Backend>, on: bool) {
    state.set_chatroom_auto_leave(on)
}

#[tauri::command]
pub fn gameroom_last_games(state: State<Backend>) -> Option<GameroomGames> {
    state.gameroom_last_games()
}

#[tauri::command]
pub async fn gameroom_postal_games(state: State<'_, Backend>) -> Result<Vec<PostalGameView>, ApiError> {
    state.gameroom_postal_games().await
}

#[tauri::command]
pub async fn invite_gameroom_player(
    state: State<'_, Backend>,
    who: String,
    side: Color,
    time_control: String,
    rated: bool,
    message: String,
) -> Result<(), ApiError> {
    state.invite_gameroom_player(&who, side, &time_control, rated, &message).await
}

#[tauri::command]
pub async fn accept_gameroom_invitation(
    state: State<'_, Backend>,
    session: SessionId,
    other_id: String,
    created: String,
) -> Result<(), ApiError> {
    state.accept_gameroom_invitation(session, &other_id, &created).await
}

#[tauri::command]
pub async fn decline_gameroom_invitation(
    state: State<'_, Backend>,
    other_id: String,
    created: String,
    reason: String,
) -> Result<(), ApiError> {
    state.decline_gameroom_invitation(&other_id, &created, &reason).await
}

#[tauri::command]
pub async fn cancel_gameroom_invitation(
    state: State<'_, Backend>,
    other_id: String,
    created: String,
) -> Result<(), ApiError> {
    state.cancel_gameroom_invitation(&other_id, &created).await
}

#[tauri::command]
pub async fn search_gameroom_players(
    state: State<'_, Backend>,
    text: String,
) -> Result<Vec<PlayerMatchView>, ApiError> {
    state.search_gameroom_players(&text).await
}

#[tauri::command]
pub async fn gameroom_player_games(
    state: State<'_, Backend>,
    player_id: String,
    offset: u32,
) -> Result<PlayerGamesView, ApiError> {
    state.gameroom_player_games(&player_id, offset).await
}

#[tauri::command]
pub async fn open_gameroom_game(
    state: State<'_, Backend>,
    session: SessionId,
    gid: String,
) -> Result<(), ApiError> {
    state.open_gameroom_game(session, &gid).await
}

#[tauri::command]
pub async fn play_gameroom_game(
    state: State<'_, Backend>,
    session: SessionId,
    gid: String,
    side: Color,
    unrated: Option<bool>,
) -> Result<(), ApiError> {
    state.play_gameroom_game(session, &gid, side, unrated.unwrap_or(false)).await
}

#[tauri::command]
pub async fn puzzle_list(
    state: State<'_, Backend>,
    refresh: Option<bool>,
) -> Result<Vec<PuzzleGroupView>, ApiError> {
    state.puzzle_list(refresh.unwrap_or(false)).await
}

#[tauri::command]
pub async fn open_puzzle(
    state: State<'_, Backend>,
    session: SessionId,
    puzzle: String,
) -> Result<(), ApiError> {
    state.open_puzzle(session, &puzzle).await
}

#[tauri::command]
pub async fn puzzle_hint(state: State<'_, Backend>, session: SessionId) -> Result<(), ApiError> {
    state.puzzle_hint(session).await
}

#[tauri::command]
pub fn puzzle_answer(state: State<Backend>, session: SessionId) -> Result<(), ApiError> {
    state.puzzle_answer(session)
}

#[tauri::command]
pub fn puzzle_retry(state: State<Backend>, session: SessionId) -> Result<(), ApiError> {
    state.puzzle_retry(session)
}

#[tauri::command]
pub async fn gameroom_server_bots(state: State<'_, Backend>) -> Result<ServerBotsView, ApiError> {
    state.gameroom_server_bots().await
}

#[tauri::command]
pub async fn gameroom_bot_info(state: State<'_, Backend>, page: String) -> Result<BotInfoView, ApiError> {
    state.gameroom_bot_info(&page).await
}

#[tauri::command]
pub async fn start_gameroom_bot(
    state: State<'_, Backend>,
    page: String,
    name: String,
    bot_side: Color,
) -> Result<StartedBotView, ApiError> {
    state.start_gameroom_bot(&page, &name, bot_side).await
}

#[tauri::command]
pub async fn create_gameroom_game(
    state: State<'_, Backend>,
    session: SessionId,
    side: Color,
    time_control: String,
    rated: bool,
) -> Result<(), ApiError> {
    state.create_gameroom_game(session, side, &time_control, rated).await
}

#[tauri::command]
pub async fn cancel_gameroom_game(state: State<'_, Backend>, gid: String) -> Result<(), ApiError> {
    state.cancel_gameroom_game(&gid).await
}

#[tauri::command]
pub async fn resign_gameroom_game(state: State<'_, Backend>, session: SessionId) -> Result<(), ApiError> {
    state.resign_gameroom_game(session).await
}

#[tauri::command]
pub async fn send_gameroom_chat(
    state: State<'_, Backend>,
    session: SessionId,
    text: String,
) -> Result<(), ApiError> {
    state.send_gameroom_chat(session, &text).await
}

#[tauri::command]
pub fn stop_watching(state: State<Backend>, session: SessionId) -> Result<(), ApiError> {
    state.stop_watching(session)
}

#[tauri::command]
pub fn watch_status(state: State<Backend>, session: SessionId) -> Result<Option<WatchView>, ApiError> {
    state.watch_status(session)
}

/// Matches the native widgets the webview leaves to the toolkit (on Linux,
/// GTK's `<select>` popups and tooltips) to the page's colors, dark or light.
#[tauri::command]
pub fn set_native_appearance(app: tauri::AppHandle, dark: bool) {
    #[cfg(target_os = "linux")]
    if let Err(e) = app.run_on_main_thread(move || gtk_appearance(dark)) {
        eprintln!("couldn't set the GTK appearance: {e}");
    }
    #[cfg(not(target_os = "linux"))]
    let _ = (app, dark);
}

/// WebKitGTK draws `<select>` popups and `title` tooltips with the GTK
/// theme, whatever the page's colors, so they can be light on a dark page
/// or the other way round. An app stylesheet over the theme gives them the
/// page's panel colors (`app.css`). Changing GTK's theme settings instead
/// would also change WebKit's `prefers-color-scheme`, and resetting them
/// doesn't bring back what the system said.
#[cfg(target_os = "linux")]
fn gtk_appearance(dark: bool) {
    use gtk::prelude::*;
    thread_local! {
        static PROVIDER: std::cell::OnceCell<gtk::CssProvider> = const { std::cell::OnceCell::new() };
    }
    // Panel, text, border and accent colors, as in app.css.
    let (bg, fg, border, accent) = if dark {
        ("#262522", "#e9e5dc", "#3a3834", "#4f86aa")
    } else {
        ("#fbfaf6", "#22201c", "#ddd6c8", "#3c6e8f")
    };
    let css = format!(
        "menu, .popup, treeview.view, treeview.view header button {{ background-color: {bg}; color: {fg}; }}\n\
         menu menuitem:hover, treeview.view:selected {{ background-color: {accent}; color: #ffffff; }}\n\
         tooltip, tooltip.background {{ background-color: {bg}; color: {fg}; border: 1px solid {border}; }}\n\
         tooltip * {{ background-color: transparent; color: {fg}; }}\n"
    );
    PROVIDER.with(|cell| {
        let provider = cell.get_or_init(|| {
            let provider = gtk::CssProvider::new();
            if let Some(screen) = gtk::gdk::Screen::default() {
                gtk::StyleContext::add_provider_for_screen(
                    &screen,
                    &provider,
                    gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
                );
            }
            provider
        });
        if let Err(e) = provider.load_from_data(css.as_bytes()) {
            eprintln!("couldn't load the GTK stylesheet: {e}");
        }
    });
}

#[tauri::command]
pub async fn tourney_status(state: State<'_, Backend>) -> Result<TourneyStatusView, ApiError> {
    Ok(state.tourney_status().await)
}

#[tauri::command]
pub async fn tourney_tests(state: State<'_, Backend>) -> Result<Vec<TestView>, ApiError> {
    state.tourney_tests().await
}

#[tauri::command]
pub async fn tourney_test(state: State<'_, Backend>, id: String) -> Result<TestDetailView, ApiError> {
    state.tourney_test(&id).await
}

#[tauri::command]
pub async fn tourney_submit(state: State<'_, Backend>, spec: NewTestSpec) -> Result<TestView, ApiError> {
    state.tourney_submit(spec).await
}

#[tauri::command]
pub async fn tourney_control(
    state: State<'_, Backend>,
    id: String,
    action: String,
) -> Result<TestView, ApiError> {
    state.tourney_control(&id, &action).await
}

#[tauri::command]
pub async fn tourney_set_priority(
    state: State<'_, Backend>,
    id: String,
    priority: i32,
) -> Result<TestView, ApiError> {
    state.tourney_set_priority(&id, priority).await
}

#[tauri::command]
pub async fn tourney_set_concurrency(
    state: State<'_, Backend>,
    id: String,
    concurrency: u32,
) -> Result<TestView, ApiError> {
    state.tourney_set_concurrency(&id, concurrency).await
}

#[tauri::command]
pub async fn tourney_set_max_pairs(
    state: State<'_, Backend>,
    id: String,
    max_pairs: Option<u32>,
) -> Result<TestView, ApiError> {
    state.tourney_set_max_pairs(&id, max_pairs).await
}

#[tauri::command]
pub async fn tourney_set_undecided_below(
    state: State<'_, Backend>,
    id: String,
    undecided_below: Option<f64>,
) -> Result<TestView, ApiError> {
    state.tourney_set_undecided_below(&id, undecided_below).await
}

#[tauri::command]
pub async fn tourney_remove(
    state: State<'_, Backend>,
    id: String,
    archive: bool,
) -> Result<Option<String>, ApiError> {
    state.tourney_remove(&id, archive).await
}

#[tauri::command]
pub async fn tourney_archived(state: State<'_, Backend>) -> Result<Vec<ArchivedTestView>, ApiError> {
    state.tourney_archived().await
}

#[tauri::command]
pub async fn tourney_restore(state: State<'_, Backend>, file: String) -> Result<TestView, ApiError> {
    state.tourney_restore(&file).await
}

#[tauri::command]
pub async fn tourney_delete_archived(state: State<'_, Backend>, file: String) -> Result<(), ApiError> {
    state.tourney_delete_archived(&file).await
}

#[tauri::command]
pub async fn tourney_set_slots(state: State<'_, Backend>, slots: u32) -> Result<(), ApiError> {
    state.tourney_set_slots(slots).await
}

#[tauri::command]
pub async fn tourney_start_daemon(state: State<'_, Backend>, slots: u32) -> Result<(), ApiError> {
    state.tourney_start_daemon(slots).await
}

#[tauri::command]
pub async fn tourney_shutdown(state: State<'_, Backend>, now: bool) -> Result<(), ApiError> {
    state.tourney_shutdown(now).await
}

#[tauri::command]
pub async fn open_tourney_game(
    state: State<'_, Backend>,
    session: SessionId,
    test: String,
    pair: u32,
    game: u8,
) -> Result<(), ApiError> {
    state.open_tourney_game(session, &test, pair, game).await
}

#[tauri::command]
pub fn engine_output(state: State<'_, Backend>, session: SessionId) -> Result<Vec<EngineOutput>, ApiError> {
    state.engine_output(session)
}

#[tauri::command]
pub async fn follow_tourney_test(
    state: State<'_, Backend>,
    session: SessionId,
    test: String,
) -> Result<bool, ApiError> {
    state.follow_tourney_test(session, &test).await
}
