//! Tauri shell over `howdah-arimaa`, `howdah-aei` and `howdah-gameroom`:
//! session state, the engine controller, followed gameroom games, commands
//! and events. Game logic lives in the core crates; this crate only adapts
//! it for the UI.

pub mod backend;
mod commands;
mod controller;
pub mod dto;
pub mod engines;
pub mod gameroom;
pub mod session;

use std::sync::Arc;

use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};

use backend::{Backend, EventSink};
use engines::EngineRegistry;
use gameroom::SavedLogin;

impl EventSink for AppHandle {
    fn emit(&self, event: &str, payload: Value) {
        if let Err(e) = Emitter::emit(self, event, payload) {
            eprintln!("failed to emit {event}: {e}");
        }
    }
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            let registry = EngineRegistry::load(config_dir.join("engines.json"));
            let saved_login = SavedLogin::new(Some(config_dir.join("gameroom-login.json")));
            app.manage(Backend::new(registry, saved_login, Arc::new(app.handle().clone())));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::open_session,
            commands::close_session,
            commands::list_sessions,
            commands::get_state,
            commands::new_game,
            commands::load_game,
            commands::export_game,
            commands::goto_ply,
            commands::goto_live,
            commands::goto_node,
            commands::goto_sibling,
            commands::goto_branch,
            commands::set_comment,
            commands::toggle_glyph,
            commands::toggle_collapsed,
            commands::promote,
            commands::demote,
            commands::make_main_line,
            commands::delete_from,
            commands::move_replay,
            commands::legal_targets,
            commands::plan_route,
            commands::try_route,
            commands::try_step,
            commands::undo_step,
            commands::cancel_turn,
            commands::commit_turn,
            commands::set_continue_turns,
            commands::take_back,
            commands::setup_swap,
            commands::commit_setup,
            commands::start_match,
            commands::end_match,
            commands::engine_move_now,
            commands::set_analysis,
            commands::add_line,
            commands::preview_line,
            commands::list_engines,
            commands::save_engine,
            commands::delete_engine,
            commands::test_engine,
            commands::gameroom_status,
            commands::gameroom_login,
            commands::gameroom_logout,
            commands::gameroom_games,
            commands::search_gameroom_players,
            commands::gameroom_player_games,
            commands::open_gameroom_game,
            commands::play_gameroom_game,
            commands::stop_watching,
            commands::watch_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
