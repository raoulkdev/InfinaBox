//! Thin Tauri commands backing the Project window's Overview and Changes
//! tabs. All the real Git logic already lives in infinabox_core and is
//! tested there — these commands only convert its `anyhow::Result` into
//! the `Result<T, String>` shape Tauri commands need.

use infinabox_core::git_indexer::{self, CommitInfo};

#[tauri::command(async)]
pub fn current_branch(path: String) -> Result<String, String> {
    git_indexer::current_branch(&path).map_err(|e| e.to_string())
}

#[tauri::command(async)]
pub fn list_recent_commits(path: String) -> Result<Vec<CommitInfo>, String> {
    git_indexer::walk_commits(&path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE_REPO: &str = "/Users/raoulkaleba/Developer/hollow-meridian-test";

    #[test]
    fn current_branch_command_delegates_correctly() {
        let branch = current_branch(FIXTURE_REPO.to_string()).expect("should resolve a branch");
        assert!(!branch.is_empty(), "branch name should not be empty");
    }

    #[test]
    fn list_recent_commits_command_delegates_correctly() {
        let commits =
            list_recent_commits(FIXTURE_REPO.to_string()).expect("should list real commits");
        assert!(!commits.is_empty(), "the fixture has real commit history");
    }

    #[test]
    fn both_commands_fail_cleanly_against_a_non_git_folder() {
        let dir = std::env::temp_dir().join(format!(
            "infinabox-overview-non-git-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let branch_result = current_branch(dir.to_string_lossy().into_owned());
        assert!(branch_result.is_err(), "a non-Git folder must not resolve a branch");

        let commits_result = list_recent_commits(dir.to_string_lossy().into_owned());
        assert!(commits_result.is_err(), "a non-Git folder must not list commits");

        std::fs::remove_dir_all(&dir).ok();
    }
}
