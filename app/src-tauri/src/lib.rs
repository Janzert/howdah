//! Tauri shell over `arimaa-core`: session state, commands and events.
//! Game logic lives in the core; this crate only adapts it for the UI.

mod commands;
pub mod dto;
pub mod session;

use commands::AppState;

pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::new_game,
            commands::load_game,
            commands::export_game,
            commands::goto_ply,
            commands::legal_targets,
            commands::try_step,
            commands::undo_step,
            commands::cancel_turn,
            commands::commit_turn,
            commands::setup_swap,
            commands::commit_setup,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
