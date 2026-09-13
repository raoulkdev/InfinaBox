mod commands;

use commands::fs::{get_default_project_path, list_directory, read_file, write_file};
use commands::project::{ask_question, refresh_project_graph};
use commands::terminal::{resize_terminal, spawn_terminal, write_to_terminal, TerminalState};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(TerminalState::default())
        .invoke_handler(tauri::generate_handler![
            list_directory,
            get_default_project_path,
            read_file,
            write_file,
            refresh_project_graph,
            ask_question,
            spawn_terminal,
            write_to_terminal,
            resize_terminal,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
