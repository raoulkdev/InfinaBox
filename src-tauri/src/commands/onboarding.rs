//! Onboarding commands: the templates, the interview's preview, and
//! creating the game (`infinabox_core::onboarding`).
//!
//! Wave 0 stub (Phase B plan, Task O2 fills it in).

use infinabox_core::onboarding::{CreatedProject, InterviewAnswers, OnboardingPreview};
use infinabox_core::scaffold::TemplateInfo;

const NOT_YET: &str = "Not implemented yet (Phase B, Task O2)";

#[tauri::command(async)]
pub fn onboarding_templates() -> Result<Vec<TemplateInfo>, String> {
    Err(NOT_YET.into())
}

#[tauri::command(async)]
pub fn onboarding_preview(answers: InterviewAnswers) -> Result<OnboardingPreview, String> {
    let _ = answers;
    Err(NOT_YET.into())
}

#[tauri::command(async)]
pub fn onboarding_create(
    parent_dir: String,
    answers: InterviewAnswers,
    template_id: String,
) -> Result<CreatedProject, String> {
    let _ = (parent_dir, answers, template_id);
    Err(NOT_YET.into())
}
