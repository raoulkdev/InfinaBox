mod commands;

use commands::agent::{
    agent_cancel, agent_send, agent_status, chat_attach, chat_create_thread, chat_list_threads,
    chat_load_thread, AgentState,
};
use commands::autofix::AutoFixState;
use commands::assets::{
    assets_credits, assets_health, assets_import, assets_read_base64, assets_scan,
};
use commands::bridge::BridgeState;
use commands::sandbox::{sandbox_apply, sandbox_discard, sandbox_list, sandbox_play};
use commands::context::{
    context_board, context_create_folder, context_delete, context_folder_icons, context_folders, context_graph, context_list,
    context_move, context_read, context_set_folder_icon, context_set_status, context_write, context_search, boards_read_all, board_write, board_delete, board_save_file, skills_list, skill_create, document_import,
};
use commands::credentials::{credential_clear, credential_set, credential_status};
use commands::generate::{generate_accept, generate_discard, generate_providers, generate_run};
use commands::journey::{journey_get, journey_set_manual};
use commands::library::{library_import, library_providers, library_search};
use commands::connect::{
    ai_providers, ai_recommended, ai_test_connection, connect_cancel, connect_resize, connect_run,
    connect_write,
};
use commands::environment::check_cli_tools;
use commands::fs::{
    create_directory, create_file, delete_path, get_default_project_path, list_directory,
    read_file, write_file,
};
use commands::overview::{current_branch, list_recent_commits};
use commands::godot::{
    game_recent_errors, game_run, game_status, game_stop, godot_install, godot_open_editor,
    godot_status, GodotState,
};
use commands::onboarding::{onboarding_create, onboarding_preview};
use commands::project::{ask_question, refresh_project_graph};
use commands::project_settings::{project_settings_get, project_settings_set};
use commands::scaffold::project_create;
use commands::settings::{app_settings_get, app_settings_set, layout_set, layouts_get};
use commands::snapshot::{snapshot_create, snapshot_list, snapshot_restore, snapshot_undo_last};
use commands::terminal::{resize_terminal, spawn_terminal, write_to_terminal, TerminalState};
use commands::watcher::{watch_project_path, WatcherState};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(TerminalState::default())
        .manage(WatcherState::default())
        .manage(AgentState::default())
        .manage(commands::credentials::SecretState::default())
        .manage(commands::generate::GenState::default())
        .manage(AutoFixState::default())
        .manage(GodotState::default())
        .manage(BridgeState::default())
        .manage(commands::connect::ConnectState::default())
        // The bridge must be listening before any agent turn launches the
        // MCP server that connects to it.
        .setup(|app| {
            commands::generate::clear_leftovers();
            commands::bridge::start(app.handle())?;
            Ok(())
        })
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
            agent_status,
            agent_send,
            chat_attach,
            agent_cancel,
            chat_create_thread,
            chat_list_threads,
            chat_load_thread,
            godot_status,
            godot_install,
            game_run,
            game_stop,
            game_status,
            game_recent_errors,
            snapshot_list,
            snapshot_create,
            snapshot_restore,
            snapshot_undo_last,
            project_create,
            ai_providers,
            ai_recommended,
            ai_test_connection,
            connect_run,
            connect_write,
            connect_resize,
            connect_cancel,
            app_settings_get,
            app_settings_set,
            layouts_get,
            layout_set,
            godot_open_editor,
            project_settings_get,
            project_settings_set,
            onboarding_preview,
            onboarding_create,
            context_list,
            context_read,
            context_write,
            context_set_status,
            context_board,
            context_graph,
            context_folders,
            context_folder_icons,
            context_set_folder_icon,
            context_create_folder,
            context_move,
            context_delete,
            context_search,
            skills_list,
            skill_create,
            sandbox_list,
            sandbox_apply,
            sandbox_discard,
            sandbox_play,
            document_import,
            boards_read_all,
            board_write,
            board_delete,
            board_save_file,
            journey_get,
            journey_set_manual,
            assets_scan,
            assets_import,
            assets_health,
            assets_credits,
            assets_read_base64,
            library_providers,
            library_search,
            library_import,
            generate_providers,
            generate_run,
            generate_accept,
            generate_discard,
            credential_status,
            credential_set,
            credential_clear,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
