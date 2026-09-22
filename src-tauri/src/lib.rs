mod commands;

use commands::environment::check_cli_tools;
use commands::fs::{
    create_directory, create_file, delete_path, get_default_project_path, list_directory,
    read_file, write_file,
};
use commands::overview::{current_branch, list_recent_commits};
use commands::project::{ask_question, refresh_project_graph};
use commands::terminal::{resize_terminal, spawn_terminal, write_to_terminal, TerminalState};
use commands::watcher::{watch_project_path, WatcherState};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(TerminalState::default())
        .manage(WatcherState::default())
        .invoke_handler(tauri::generate_handler![
            list_directory,
            get_default_project_path,
            read_file,
            write_file,
            create_file,
            create_directory,
            delete_path,
            refresh_project_graph,
            ask_question,
            spawn_terminal,
            write_to_terminal,
            resize_terminal,
            current_branch,
            list_recent_commits,
            check_cli_tools,
            watch_project_path,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
