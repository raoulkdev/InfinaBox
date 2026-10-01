//! Onboarding commands: the templates, the interview's preview, and
//! creating the game (`infinabox_core::onboarding`).
//!
//! Thin wrappers: the real work (the foundation, the starter Context, the
//! one first snapshot, cleanup on failure) lives in core, whose errors are
//! plain sentences written for users, so they're passed through whole —
//! cause chain included via `{:#}`. Creating a game starts no AI turn and no
//! chat: the game is planned and built with the developer afterwards.

use std::path::Path;

use infinabox_core::onboarding::{self, CreatedProject, InterviewAnswers, OnboardingPreview};
#[cfg(test)]
use infinabox_core::scaffold;

/// Formats a core error with its whole cause chain, like `snapshot.rs`.
fn user_error(e: impl std::fmt::Display) -> String {
    format!("{e:#}")
}

fn preview(answers: &InterviewAnswers) -> Result<OnboardingPreview, String> {
    onboarding::preview(answers).map_err(user_error)
}

fn create(parent_dir: &str, answers: &InterviewAnswers) -> Result<CreatedProject, String> {
    onboarding::create_from_interview(Path::new(parent_dir), answers).map_err(user_error)
}

#[tauri::command(async)]
pub fn onboarding_preview(answers: InterviewAnswers) -> Result<OnboardingPreview, String> {
    preview(&answers)
}

#[tauri::command(async)]
pub fn onboarding_create(parent_dir: String, answers: InterviewAnswers) -> Result<CreatedProject, String> {
    create(&parent_dir, &answers)
}

#[cfg(test)]
mod tests {
    use super::*;
    use infinabox_core::agent::TechnicalLevel;
    use infinabox_core::{chat_store, snapshot};
    use std::fs;
    use std::path::PathBuf;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "infinabox-onboarding-cmd-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn answers(name: &str) -> InterviewAnswers {
        InterviewAnswers {
            idea: "A fox who jumps between rooftops collecting lost letters".into(),
            genre: "2d".into(),
            references: "Celeste".into(),
            technical_level: TechnicalLevel::Balanced,
            name: name.into(),
        }
    }

    #[test]
    fn lists_the_foundations_and_the_older_templates() {
        let ids: Vec<String> = scaffold::list_templates().into_iter().map(|t| t.id).collect();
        for id in ["foundation-2d", "foundation-3d", "platformer-2d", "explorer-3d", "blank-2d"] {
            assert!(ids.iter().any(|i| i == id), "missing {id} in {ids:?}");
        }
    }

    #[test]
    fn preview_picks_the_foundation_and_rejects_bad_names() {
        let p = preview(&answers("Rooftop Fox")).unwrap();
        assert_eq!(p.choice.template_id, "foundation-2d");
        assert_eq!(p.project_name, "Rooftop Fox");
        assert!(p.cards.iter().any(|c| c == "concept.md"));
        assert!(!p.setup_steps.is_empty());

        let err = preview(&answers(".hidden")).unwrap_err();
        assert_eq!(err, "the project name can't start with a dot");
    }

    /// The path the Create button takes: a foundation, no chat, one snapshot.
    #[test]
    fn creates_a_foundation_game_and_starts_nothing() {
        let parent = temp_dir("create");
        let created = create(&parent.to_string_lossy(), &answers("Rooftop Fox")).unwrap();

        let project = Path::new(&created.path);
        assert_eq!(project, parent.join("Rooftop Fox"));
        assert!(project.join(".ibproject/.ibx").is_file());
        assert!(project.join("project.godot").is_file());
        let concept = fs::read_to_string(project.join(".ibproject/context/concept.md")).unwrap();
        assert!(concept.contains("A fox who jumps between rooftops"), "{concept}");
        assert!(project.join(".ibproject/context/style-guide.md").is_file());
        assert!(chat_store::list_threads(project).unwrap().is_empty());

        let history = snapshot::list_snapshots(project, 10).unwrap();
        let titles: Vec<&str> = history.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, ["New game: Rooftop Fox"]);

        // A second game with the same name is refused, plainly, and the
        // first one is left alone.
        let err = create(&parent.to_string_lossy(), &answers("Rooftop Fox")).unwrap_err();
        assert!(!err.is_empty());
        assert!(project.join(".ibproject/.ibx").is_file());
        fs::remove_dir_all(&parent).unwrap();
    }

    #[test]
    fn a_missing_dimension_is_refused_before_anything_is_written() {
        let parent = temp_dir("unknown");
        let mut a = answers("Nope");
        a.genre = String::new();
        let err = create(&parent.to_string_lossy(), &a).unwrap_err();
        assert!(!err.is_empty());
        assert!(!parent.join("Nope").exists());
        fs::remove_dir_all(&parent).unwrap();
    }
}
