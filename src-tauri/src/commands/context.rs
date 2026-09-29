//! Context card commands (`infinabox_core::context_cards`).
//!
//! Wave 0 stubs — task X1 fills in the bodies.

use infinabox_core::context_cards::{Board, Card, CardMeta, CardSummary, CardType, LinkGraph};

const NOT_YET: &str = "Not implemented yet (Phase C, task X1)";

#[tauri::command(async)]
pub fn context_list(project_path: String) -> Result<Vec<CardSummary>, String> {
    let _ = project_path;
    Err(NOT_YET.into())
}

#[tauri::command(async)]
pub fn context_read(project_path: String, path: String) -> Result<Card, String> {
    let _ = (project_path, path);
    Err(NOT_YET.into())
}

#[tauri::command(async)]
pub fn context_write(
    project_path: String,
    path: String,
    meta: CardMeta,
    body: String,
) -> Result<Card, String> {
    let _ = (project_path, path, meta, body);
    Err(NOT_YET.into())
}

#[tauri::command(async)]
pub fn context_set_status(project_path: String, path: String, status: String) -> Result<Card, String> {
    let _ = (project_path, path, status);
    Err(NOT_YET.into())
}

#[tauri::command(async)]
pub fn context_board(project_path: String, types: Vec<CardType>) -> Result<Board, String> {
    let _ = (project_path, types);
    Err(NOT_YET.into())
}

#[tauri::command(async)]
pub fn context_graph(project_path: String) -> Result<LinkGraph, String> {
    let _ = project_path;
    Err(NOT_YET.into())
}
