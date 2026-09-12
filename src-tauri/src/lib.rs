mod commands;

use commands::fs::{get_default_project_path, list_directory, read_file, write_file};
use commands::project::{ask_question, refresh_project_graph};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            list_directory,
            get_default_project_path,
            read_file,
            write_file,
            refresh_project_graph,
            ask_question,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
