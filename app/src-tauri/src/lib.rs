//! Tauri shell over `howdah-arimaa`, `howdah-aei` and `howdah-gameroom`:
//! session state, the engine controller, followed gameroom games, commands
//! and events. Game logic lives in the core crates; this crate only adapts
//! it for the UI.

pub mod backend;
mod commands;
mod controller;
pub mod dto;
pub mod engine_install;
pub mod engines;
pub mod gameroom;
pub mod session;

use std::sync::Arc;

use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, WindowEvent};

use backend::{Backend, EventSink};
use engine_install::EngineCatalog;
use engines::EngineRegistry;
use gameroom::SavedLogin;

impl EventSink for AppHandle {
    fn emit(&self, event: &str, payload: Value) {
        if let Err(e) = Emitter::emit(self, event, payload) {
            eprintln!("failed to emit {event}: {e}");
        }
    }
}

/// How many game windows are open, leaving out the window `except`.
fn game_windows(app: &AppHandle, except: Option<&str>) -> usize {
    app.webview_windows()
        .keys()
        .filter(|label| backend::game_window_session(label).is_some() && Some(label.as_str()) != except)
        .count()
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            let registry = EngineRegistry::load(config_dir.join("engines.json"));
            let catalog = EngineCatalog::load(
                config_dir.join("engine-manifests.json"),
                app.path().app_local_data_dir()?.join("engines"),
            );
            let saved_login = SavedLogin::new(Some(config_dir.join("gameroom-login.json")));
            app.manage(Backend::new(registry, catalog, saved_login, Arc::new(app.handle().clone())));
            Ok(())
        })
        .on_window_event(|window, event| match event {
            // Closing the lobby (the main window) only hides it while game
            // windows are open, so its lobby poll and notifications go on;
            // a game window's Lobby button shows it again.
            WindowEvent::CloseRequested { api, .. } if window.label() == "main" => {
                if game_windows(window.app_handle(), None) > 0 {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
            // A game window's session ends with the window, however it
            // closed. Closing the last one while the lobby is hidden quits.
            WindowEvent::Destroyed => {
                window.state::<Backend>().window_closed(window.label());
                let app = window.app_handle();
                let lobby_hidden =
                    app.get_webview_window("main").is_some_and(|w| !w.is_visible().unwrap_or(true));
                if lobby_hidden && game_windows(app, Some(window.label())) == 0 {
                    app.exit(0);
                }
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            commands::open_session,
            commands::close_session,
            commands::list_sessions,
            commands::get_state,
            commands::new_game,
            commands::load_game,
            commands::new_game_from,
            commands::editor_position,
            commands::parse_position,
            commands::check_position,
            commands::edit_position,
            commands::default_position,
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
            commands::request_takeback,
            commands::answer_takeback,
            commands::setup_swap,
            commands::commit_setup,
            commands::start_match,
            commands::end_match,
            commands::engine_move_now,
            commands::set_engine_options,
            commands::set_analysis_options,
            commands::press_engine_button,
            commands::set_analysis,
            commands::add_line,
            commands::preview_line,
            commands::list_engines,
            commands::save_engine,
            commands::delete_engine,
            commands::test_engine,
            commands::engine_catalog,
            commands::add_engine_manifest,
            commands::remove_engine_manifest,
            commands::refresh_engine_manifest,
            commands::install_engine,
            commands::gameroom_status,
            commands::gameroom_login,
            commands::gameroom_logout,
            commands::gameroom_games,
            commands::gameroom_last_games,
            commands::gameroom_game_session,
            commands::gameroom_postal_games,
            commands::invite_gameroom_player,
            commands::accept_gameroom_invitation,
            commands::decline_gameroom_invitation,
            commands::cancel_gameroom_invitation,
            commands::search_gameroom_players,
            commands::gameroom_player_games,
            commands::open_gameroom_game,
            commands::play_gameroom_game,
            commands::gameroom_server_bots,
            commands::gameroom_bot_info,
            commands::start_gameroom_bot,
            commands::create_gameroom_game,
            commands::cancel_gameroom_game,
            commands::resign_gameroom_game,
            commands::send_gameroom_chat,
            commands::stop_watching,
            commands::watch_status,
            commands::set_native_appearance,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app, _event| {
            // The dock icon brings back a hidden lobby.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = _event
                && let Some(lobby) = _app.get_webview_window("main")
            {
                let _ = lobby.show();
                let _ = lobby.set_focus();
            }
        });
}
