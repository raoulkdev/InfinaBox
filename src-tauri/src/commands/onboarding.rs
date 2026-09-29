//! Onboarding commands: the templates, the interview's preview, and
//! creating the game (`infinabox_core::onboarding`).
//!
//! Thin wrappers: the real work (template choice, starter Context, the
//! "First build" thread, the one first snapshot, cleanup on failure) lives
//! in core, whose errors are plain sentences written for users, so they're
//! passed through whole — cause chain included via `{:#}`.

use std::path::Path;

use infinabox_core::app_settings;
use infinabox_core::connect::ProviderId;
use infinabox_core::onboarding::{self, CreatedProject, InterviewAnswers, OnboardingPreview};
use infinabox_core::scaffold::{self, TemplateInfo};
use tauri::{AppHandle, Manager};

/// Formats a core error with its whole cause chain, like `snapshot.rs`.
fn user_error(e: impl std::fmt::Display) -> String {
    format!("{e:#}")
}

/// The AI the new game's first thread is for: the one chosen in
/// `<app_data>/settings.json`, else Claude Code. Settings that can't be
/// read (missing folder, damaged file) fall back to the default rather than
/// failing the game's creation — the thread's provider is switched later
/// anyway if the person picks another AI.
fn selected_provider(app_data: Option<&Path>) -> &'static str {
    app_data
        .and_then(|dir| app_settings::load(dir).ok())
        .and_then(|settings| settings.ai_provider)
        .unwrap_or(ProviderId::ClaudeCode)
        .as_str()
}

fn templates() -> Vec<TemplateInfo> {
    scaffold::list_templates()
}

fn preview(answers: &InterviewAnswers) -> Result<OnboardingPreview, String> {
    onboarding::preview(answers).map_err(user_error)
}

fn create(
    parent_dir: &str,
    answers: &InterviewAnswers,
    template_id: &str,
    app_data: Option<&Path>,
) -> Result<CreatedProject, String> {
    onboarding::create_from_interview(
        Path::new(parent_dir),
        answers,
        template_id,
        selected_provider(app_data),
    )
    .map_err(user_error)
}

#[tauri::command(async)]
pub fn onboarding_templates() -> Result<Vec<TemplateInfo>, String> {
    Ok(templates())
}

#[tauri::command(async)]
pub fn onboarding_preview(answers: InterviewAnswers) -> Result<OnboardingPreview, String> {
    preview(&answers)
}

#[tauri::command(async)]
pub fn onboarding_create(
    app: AppHandle,
    parent_dir: String,
    answers: InterviewAnswers,
    template_id: String,
) -> Result<CreatedProject, String> {
    let app_data = app.path().app_data_dir().ok();
    create(&parent_dir, &answers, &template_id, app_data.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use infinabox_core::app_settings::AppSettings;
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

    fn platformer_answers(name: &str) -> InterviewAnswers {
        InterviewAnswers {
            idea: "A fox who jumps between rooftops collecting lost letters".into(),
            genre: "platformer-2d".into(),
            genre_other: None,
            feel: vec!["cozy".into(), "bouncy".into()],
            look: "soft pastel colors".into(),
            references: "Celeste".into(),
            session_length: "10 minutes".into(),
            name: name.into(),
        }
    }

    #[test]
    fn provider_defaults_to_claude_code_without_settings() {
        assert_eq!(selected_provider(None), "claude-code");
        let empty = temp_dir("no-settings");
        assert_eq!(selected_provider(Some(&empty)), "claude-code");
        assert_eq!(selected_provider(Some(&empty.join("missing"))), "claude-code");
        fs::remove_dir_all(&empty).unwrap();
    }

    #[test]
    fn provider_comes_from_the_app_settings() {
        let dir = temp_dir("settings");
        let settings = AppSettings {
            ai_provider: Some(ProviderId::Codex),
            ..Default::default()
        };
        app_settings::save(&dir, &settings).unwrap();
        assert_eq!(selected_provider(Some(&dir)), "codex");

        app_settings::save(&dir, &AppSettings::default()).unwrap();
        assert_eq!(selected_provider(Some(&dir)), "claude-code");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn damaged_settings_fall_back_to_the_default() {
        let dir = temp_dir("damaged");
        fs::write(dir.join("settings.json"), "{ not json").unwrap();
        assert!(app_settings::load(&dir).is_err());
        assert_eq!(selected_provider(Some(&dir)), "claude-code");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn lists_the_real_templates() {
        let ids: Vec<String> = templates().into_iter().map(|t| t.id).collect();
        for id in [
            "platformer-2d",
            "topdown-2d",
            "shooter-2d",
            "puzzle-2d",
            "visualnovel-2d",
            "explorer-3d",
            "firstperson-3d",
            "blank-2d",
        ] {
            assert!(ids.iter().any(|i| i == id), "missing {id} in {ids:?}");
        }
    }

    #[test]
    fn preview_picks_the_chosen_template_and_rejects_bad_names() {
        let p = preview(&platformer_answers("Rooftop Fox")).unwrap();
        assert_eq!(p.choice.template_id, "platformer-2d");
        assert_eq!(p.project_name, "Rooftop Fox");
        assert!(p.cards.iter().any(|c| c == "concept.md"));
        assert!(!p.first_build_steps.is_empty());

        let err = preview(&platformer_answers(".hidden")).unwrap_err();
        assert_eq!(err, "the project name can't start with a dot");
    }

    /// The end-to-end path the Create button takes: a platformer from
    /// sample answers, with Codex selected in the app settings.
    #[test]
    fn creates_a_platformer_game_for_the_selected_ai() {
        let parent = temp_dir("create");
        let app_data = temp_dir("create-appdata");
        app_settings::save(
            &app_data,
            &AppSettings {
                ai_provider: Some(ProviderId::Codex),
                ..Default::default()
            },
        )
        .unwrap();

        let created = create(
            &parent.to_string_lossy(),
            &platformer_answers("Rooftop Fox"),
            "platformer-2d",
            Some(&app_data),
        )
        .unwrap();

        let project = Path::new(&created.path);
        assert_eq!(project, parent.join("Rooftop Fox"));
        assert!(project.join(".ibproject/.ibx").is_file());
        assert!(project.join("project.godot").is_file());
        let concept = fs::read_to_string(project.join(".ibproject/context/concept.md")).unwrap();
        assert!(concept.contains("A fox who jumps between rooftops"), "{concept}");
        assert!(project.join(".ibproject/context/style-guide.md").is_file());
        assert!(project.join(".ibproject/context/tasks/first-playable.md").is_file());
        assert!(!created.first_build_message.trim().is_empty());

        let threads = chat_store::list_threads(project).unwrap();
        let thread = threads.iter().find(|t| t.id == created.thread_id).unwrap();
        assert_eq!(thread.provider, "codex");

        // History starts as exactly one snapshot, holding all of the above.
        let history = snapshot::list_snapshots(project, 10).unwrap();
        let titles: Vec<&str> = history.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, ["New game: Rooftop Fox"]);

        // A second game with the same name is refused, plainly, and the
        // first one is left alone.
        let err = create(
            &parent.to_string_lossy(),
            &platformer_answers("Rooftop Fox"),
            "platformer-2d",
            Some(&app_data),
        )
        .unwrap_err();
        assert!(!err.is_empty());
        assert!(project.join(".ibproject/.ibx").is_file());

        fs::remove_dir_all(&parent).unwrap();
        fs::remove_dir_all(&app_data).unwrap();
    }

    #[test]
    fn an_unknown_template_is_refused_before_anything_is_written() {
        let parent = temp_dir("unknown");
        let err = create(
            &parent.to_string_lossy(),
            &platformer_answers("Nope"),
            "racing-3d",
            None,
        )
        .unwrap_err();
        assert!(!err.is_empty());
        assert!(!parent.join("Nope").exists());
        fs::remove_dir_all(&parent).unwrap();
    }
}
