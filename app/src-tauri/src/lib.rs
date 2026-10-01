//! Tauri shell over `arimaa-core` and `arimaa-aei`: session state, the
//! engine controller, commands and events. Game logic lives in the core
//! crates; this crate only adapts it for the UI.

mod commands;
mod controller;
pub mod dto;
pub mod engines;
pub mod session;

use std::sync::{Arc, Mutex};

use tauri::Manager;

use commands::AppState;
use engines::EngineRegistry;
use session::Session;

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            let engines = Arc::new(Mutex::new(EngineRegistry::load(config_dir.join("engines.json"))));
            let session = Arc::new(Mutex::new(Session::new()));
            let controller = controller::spawn(app.handle().clone(), session.clone(), engines.clone());
            app.manage(AppState { session, engines, controller });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::new_game,
            commands::load_game,
            commands::export_game,
            commands::goto_ply,
            commands::legal_targets,
            commands::plan_route,
            commands::try_route,
            commands::try_step,
            commands::undo_step,
            commands::cancel_turn,
            commands::commit_turn,
            commands::setup_swap,
            commands::commit_setup,
            commands::start_match,
            commands::end_match,
            commands::engine_move_now,
            commands::list_engines,
            commands::save_engine,
            commands::delete_engine,
            commands::test_engine,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
