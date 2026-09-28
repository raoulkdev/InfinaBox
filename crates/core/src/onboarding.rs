//! The first-run interview (spec §6.2): turns the person's answers into a
//! template choice with a plain reason, a starter Context, a new project,
//! and the message for its first build.
//!
//! Wave 0 stub (Phase B plan, Task OB fills it in).

use std::path::Path;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use crate::scaffold::TemplateInfo;

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct InterviewAnswers {
    /// "What's your game about?" in their own words.
    pub idea: String,
    /// A template id, or "other".
    pub genre: String,
    /// Their own description when `genre` is "other".
    pub genre_other: Option<String>,
    /// How it should feel ("cozy", "fast", ...), chips or their own words.
    pub feel: Vec<String>,
    /// How it should look ("pixel art", ...).
    pub look: String,
    /// Games it's like (optional, may be empty).
    pub references: String,
    /// How long one play session is.
    pub session_length: String,
    /// The game's name (also the project folder name).
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TemplateChoice {
    pub template_id: String,
    /// Why this template, in plain words.
    pub reason: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct OnboardingPreview {
    pub choice: TemplateChoice,
    pub project_name: String,
    /// Context cards that will be written, relative to `.ibproject/context/`.
    pub cards: Vec<String>,
    /// "What I'll build first", in plain words.
    pub first_build_steps: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CreatedProject {
    pub path: String,
    /// The "First build" chat thread.
    pub thread_id: String,
    pub first_build_message: String,
}

pub fn choose_template(answers: &InterviewAnswers, templates: &[TemplateInfo]) -> TemplateChoice {
    let _ = answers;
    TemplateChoice {
        template_id: templates.first().map(|t| t.id.clone()).unwrap_or_default(),
        reason: String::new(),
    }
}

pub fn preview(answers: &InterviewAnswers) -> Result<OnboardingPreview> {
    let _ = answers;
    bail!("not implemented yet")
}

/// Scaffolds from the chosen template, writes the starter Context, creates
/// the "First build" chat thread (for `provider`), snapshots, and returns
/// the first build message.
pub fn create_from_interview(
    parent: &Path,
    answers: &InterviewAnswers,
    template_id: &str,
    provider: &str,
) -> Result<CreatedProject> {
    let _ = (parent, answers, template_id, provider);
    bail!("not implemented yet")
}
