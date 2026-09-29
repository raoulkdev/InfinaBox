//! Context card commands (`infinabox_core::context_cards`).
//!

use std::path::Path;

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
