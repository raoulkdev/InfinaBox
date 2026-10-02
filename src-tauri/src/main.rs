// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // The same executable doubles as the InfinaBox MCP server: the agent CLI
    // launches `<this exe> --mcp-server` (see `commands::agent`), which must
    // never open a window — so this check runs before Tauri starts at all.
    if std::env::args().nth(1).as_deref() == Some(infinabox_mcp_server::MCP_SERVER_FLAG) {
        if let Err(e) = infinabox_mcp_server::run_stdio() {
            eprintln!("infinabox mcp server: {e:#}");
            std::process::exit(1);
        }
        return;
    }
    tauri_app_lib::run()
}
