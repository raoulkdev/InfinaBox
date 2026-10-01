//! Context card commands (`infinabox_core::context_cards`).
//!

use std::path::Path;

use infinabox_core::boards;
use infinabox_core::context_cards::{self, Board, Card, CardMeta, CardSummary, CardType, LinkGraph};

fn err(e: anyhow::Error) -> String {
    format!("{e:#}")
}

#[tauri::command(async)]
pub fn context_list(project_path: String) -> Result<Vec<CardSummary>, String> {
    context_cards::list_cards(Path::new(&project_path)).map_err(err)
}

#[tauri::command(async)]
pub fn context_read(project_path: String, path: String) -> Result<Card, String> {
    context_cards::read_card(Path::new(&project_path), &path).map_err(err)
}

#[tauri::command(async)]
pub fn context_write(
    project_path: String,
    path: String,
    meta: CardMeta,
    body: String,
) -> Result<Card, String> {
    context_cards::write_card(Path::new(&project_path), &path, &meta, &body).map_err(err)
}

#[tauri::command(async)]
pub fn context_set_status(project_path: String, path: String, status: String) -> Result<Card, String> {
    context_cards::set_status(Path::new(&project_path), &path, &status).map_err(err)
}

#[tauri::command(async)]
pub fn context_board(project_path: String, types: Vec<CardType>) -> Result<Board, String> {
    context_cards::board(Path::new(&project_path), &types).map_err(err)
}

#[tauri::command(async)]
pub fn context_graph(project_path: String) -> Result<LinkGraph, String> {
    context_cards::graph(Path::new(&project_path)).map_err(err)
}

#[tauri::command(async)]
pub fn context_folders(project_path: String) -> Result<Vec<String>, String> {
    context_cards::list_folders(Path::new(&project_path)).map_err(err)
}

#[tauri::command(async)]
pub fn context_create_folder(project_path: String, path: String) -> Result<(), String> {
    context_cards::create_folder(Path::new(&project_path), &path).map_err(err)
}

/// Moves or renames a note or folder; returns the new path.
#[tauri::command(async)]
pub fn context_move(project_path: String, from: String, to: String) -> Result<String, String> {
    context_cards::move_entry(Path::new(&project_path), &from, &to).map_err(err)
}

#[tauri::command(async)]
pub fn context_delete(project_path: String, path: String) -> Result<(), String> {
    context_cards::delete_entry(Path::new(&project_path), &path).map_err(err)
}

#[tauri::command(async)]
pub fn context_folder_icons(project_path: String) -> Result<std::collections::BTreeMap<String, String>, String> {
    context_cards::folder_icons(Path::new(&project_path)).map_err(err)
}

#[tauri::command(async)]
pub fn context_set_folder_icon(project_path: String, path: String, icon: Option<String>) -> Result<(), String> {
    context_cards::set_folder_icon(Path::new(&project_path), &path, icon.as_deref()).map_err(err)
}

#[tauri::command(async)]
pub fn context_search(
    project_path: String,
    query: String,
) -> Result<Vec<context_cards::CardHit>, String> {
    context_cards::search(Path::new(&project_path), &query, 40).map_err(err)
}

// ---- Document boards (`infinabox_core::boards`) ----

#[tauri::command(async)]
pub fn boards_read_all(project_path: String) -> Result<Vec<boards::StoredBoard>, String> {
    boards::read_all(Path::new(&project_path)).map_err(err)
}

#[tauri::command(async)]
pub fn board_write(project_path: String, id: String, json: String) -> Result<(), String> {
    boards::write(Path::new(&project_path), &id, &json).map_err(err)
}

#[tauri::command(async)]
pub fn board_delete(project_path: String, id: String) -> Result<(), String> {
    boards::delete(Path::new(&project_path), &id).map_err(err)
}

/// Saves a file dropped, pasted or picked on a board; returns its project path.
#[tauri::command(async)]
pub fn board_save_file(project_path: String, name: String, data_base64: String) -> Result<String, String> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data_base64.as_bytes())
        .map_err(|_| "That file couldn't be read.".to_string())?;
    boards::save_file(Path::new(&project_path), &name, &bytes).map_err(err)
}
