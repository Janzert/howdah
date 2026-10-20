//! Tauri shell over `howdah-arimaa`, `howdah-aei`, `howdah-gameroom` and
//! `howdah-tourney`'s client:
//! session state, the engine controller, followed gameroom games, commands
//! and events. Game logic lives in the core crates; this crate only adapts
//! it for the UI.

pub mod backend;
mod chat_archive;
mod chatroom;
mod commands;
mod controller;
pub mod dto;
pub mod engine_install;
pub mod engines;
pub mod gameroom;
mod puzzles;
pub mod session;
pub mod tourney;

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
/// Tells the hidden lobby that the last game window closed while boards
/// are docked in it: it quits, or shows itself and asks first.
const LAST_WINDOW: &str = "lobby://last-window-closed";

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
            let archive_dir = app.path().app_local_data_dir()?.join("chat-archive");
            app.manage(
                Backend::new(registry, catalog, saved_login, Arc::new(app.handle().clone()))
                    .with_chat_archive_dir(Some(archive_dir)),
            );
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
            // closed. Closing the last one while the lobby is hidden quits,
            // unless boards are docked there: then the lobby decides, and
            // asks first if any of them is still playing (`LAST_WINDOW`).
            WindowEvent::Destroyed => {
                let backend = window.state::<Backend>();
                backend.window_closed(window.label());
                let app = window.app_handle();
                let lobby_hidden =
                    app.get_webview_window("main").is_some_and(|w| !w.is_visible().unwrap_or(true));
                if lobby_hidden && game_windows(app, Some(window.label())) == 0 {
                    if backend.docked_sessions().is_empty() {
                        app.exit(0);
                    } else if let Err(e) = Emitter::emit(app, LAST_WINDOW, ()) {
                        eprintln!("failed to emit {LAST_WINDOW}: {e}");
                        app.exit(0);
                    }
                }
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            commands::open_session,
            commands::close_session,
            commands::list_sessions,
            commands::docked_sessions,
            commands::set_session_docked,
            commands::show_board,
            commands::get_state,
            commands::new_game,
            commands::load_game,
            commands::new_game_from,
            commands::editor_position,
            commands::parse_position,
            commands::check_position,
            commands::edit_position,
            commands::default_position,
            commands::puzzle_list,
            commands::open_puzzle,
            commands::puzzle_hint,
            commands::puzzle_answer,
            commands::puzzle_retry,
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
            commands::open_url,
            commands::chatroom_state,
            commands::chat_archive_month,
            commands::chat_archive_day,
            commands::enter_chatroom,
            commands::leave_chatroom,
            commands::format_chat_message,
            commands::say_in_chatroom,
            commands::set_chatroom_active,
            commands::set_chatroom_auto_leave,
            commands::gameroom_game_session,
            commands::gameroom_postal_games,
            commands::invite_gameroom_player,
            commands::accept_gameroom_invitation,
            commands::decline_gameroom_invitation,
            commands::cancel_gameroom_invitation,
            commands::search_gameroom_players,
            commands::gameroom_player_games,
            commands::open_gameroom_game,
            commands::tourney_status,
            commands::tourney_tests,
            commands::tourney_test,
            commands::tourney_submit,
            commands::tourney_control,
            commands::tourney_set_priority,
            commands::tourney_set_max_pairs,
            commands::tourney_set_undecided_below,
            commands::tourney_remove,
            commands::tourney_archived,
            commands::tourney_restore,
            commands::tourney_delete_archived,
            commands::tourney_set_concurrency,
            commands::tourney_set_slots,
            commands::tourney_start_daemon,
            commands::tourney_shutdown,
            commands::tourney_cancel_shutdown,
            commands::open_tourney_game,
            commands::follow_tourney_test,
            commands::engine_output,
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
            // Say goodbye to the gameroom's chat room, rather than being
            // timed out of it a minute later (both are in its log).
            if let tauri::RunEvent::Exit = _event {
                let backend = _app.state::<Backend>();
                // The timeout is made inside the runtime: a tokio timer
                // made outside one panics.
                let _ = tauri::async_runtime::block_on(async {
                    tokio::time::timeout(std::time::Duration::from_secs(2), backend.leave_chatroom()).await
                });
            }
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
