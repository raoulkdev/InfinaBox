//! Detects which command-line tools are actually available on the user's
//! machine. InfinaBox never manages agent CLI installs or logins itself —
//! the embedded terminal (see `commands::terminal`) just runs the user's
//! own shell, so `claude`/`codex`/whatever they use is already installed
//! and authenticated outside this app. This command exists so the
//! dashboard can honestly reflect that state (real PATH lookups) rather
//! than showing "Connected"/"Install" controls for accounts InfinaBox has
//! no way to actually manage.

use std::path::PathBuf;

use serde::Serialize;

#[derive(Serialize, Clone)]
pub struct CliToolStatus {
    pub name: String,
    pub installed: bool,
}

/// True if `name` resolves to an executable file somewhere on `PATH` —
/// the same resolution order a shell uses to find a command, without
/// spawning a `which`/`where` subprocess (which isn't guaranteed to exist
/// on every platform this could run on).
fn is_on_path(name: &str) -> bool {
    let Some(path_var) = std::env::var_os("PATH") else {
        return false;
    };

    std::env::split_paths(&path_var).any(|dir| {
        let candidate: PathBuf = dir.join(name);
        if candidate.is_file() {
            return true;
        }
        cfg!(windows) && dir.join(format!("{name}.exe")).is_file()
    })
}

#[tauri::command]
pub fn check_cli_tools(names: Vec<String>) -> Vec<CliToolStatus> {
    names
        .into_iter()
        .map(|name| {
            let installed = is_on_path(&name);
            CliToolStatus { name, installed }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_a_tool_that_is_really_on_path() {
        // `cargo` itself must be on PATH in any environment these tests run
        // in — a real, unfakeable positive case.
        let results = check_cli_tools(vec!["cargo".to_string()]);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "cargo");
        assert!(results[0].installed, "cargo should resolve on PATH");
    }

    #[test]
    fn reports_false_for_a_name_that_cannot_exist() {
        let results =
            check_cli_tools(vec!["definitely-not-a-real-cli-tool-xyz123".to_string()]);
        assert_eq!(results.len(), 1);
        assert!(!results[0].installed);
    }

    #[test]
    fn preserves_input_order_and_count_for_multiple_names() {
        let results = check_cli_tools(vec![
            "cargo".to_string(),
            "definitely-not-a-real-cli-tool-xyz123".to_string(),
        ]);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].name, "cargo");
        assert_eq!(results[1].name, "definitely-not-a-real-cli-tool-xyz123");
    }
}
