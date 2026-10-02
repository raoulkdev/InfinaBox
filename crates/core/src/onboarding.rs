//! The new-game interview (spec §6.2): turns a few answers into a project
//! on an organised foundation, a starter Context, and the person's choice of
//! how technical the conversation is.
//!
//! A game made here is a **full-size game's** starting point, not a quick
//! prototype: the project holds the systems every big game needs (game flow,
//! scene routing, saving, settings, an event bus) and no gameplay at all.
//! Nothing is built when it is created; the AI and the developer plan and
//! build it together from the first chat message on.
//!
//! Everything here is deterministic: the foundation is the person's own 2D or
//! 3D pick, the starter cards are built from their own words plus the
//! template's cards, and nothing is invented to fill a gap — an answer left
//! empty is simply left out.
//!
//! A new game's history starts as **one** snapshot, "New game: <name>".

use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::agent::TechnicalLevel;
use crate::project_settings::{self, ProjectSettings};
use crate::scaffold::{self, FOUNDATION_2D_ID, FOUNDATION_3D_ID};

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct InterviewAnswers {
    /// "What's your game about?" in their own words (may be empty).
    pub idea: String,
    /// "2d" or "3d".
    pub genre: String,
    /// Games it's like (optional, may be empty).
    pub references: String,
    /// How technical the AI's explanations should be.
    pub technical_level: TechnicalLevel,
    /// The game's name (also the project folder name).
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TemplateChoice {
    pub template_id: String,
    /// Why this foundation, in plain words.
    pub reason: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct OnboardingPreview {
    pub choice: TemplateChoice,
    pub project_name: String,
    /// Context cards that will be written, relative to `.ibproject/context/`.
    pub cards: Vec<String>,
    /// What gets set up, in plain words (first person).
    pub setup_steps: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CreatedProject {
    pub path: String,
}

/// The `genre` answers of the "2D or 3D" question.
pub const GENRE_2D: &str = "2d";
pub const GENRE_3D: &str = "3d";

/// The starter cards InfinaBox writes, relative to `.ibproject/context/`.
/// They replace the template's own placeholders; the template's systems and
/// task cards stay.
const CONCEPT_CARD: &str = "concept.md";
const STYLE_GUIDE_CARD: &str = "style-guide.md";
const STARTER_CARDS: [&str; 2] = [CONCEPT_CARD, STYLE_GUIDE_CARD];

/// The foundation for the person's 2D or 3D pick.
pub fn choose_template(answers: &InterviewAnswers) -> Result<TemplateChoice> {
    match answers.genre.trim() {
        GENRE_2D => Ok(TemplateChoice {
            template_id: FOUNDATION_2D_ID.into(),
            reason: "A 2D base with the systems every big game needs: menu, game flow, saving \
                     and settings. No gameplay yet, on purpose."
                .into(),
        }),
        GENRE_3D => Ok(TemplateChoice {
            template_id: FOUNDATION_3D_ID.into(),
            reason: "A 3D base with the systems every big game needs: menu, game flow, saving \
                     and settings. No gameplay yet, on purpose."
                .into(),
        }),
        _ => bail!("Choose whether your game is 2D or 3D."),
    }
}

/// What the review screen shows before anything is created. Checks the
/// name the same way creating the project will; never touches the disk.
pub fn preview(answers: &InterviewAnswers) -> Result<OnboardingPreview> {
    scaffold::validate_name(&answers.name)?;
    let choice = choose_template(answers)?;
    let (template, _) = scaffold::find_template(&choice.template_id)?;

    let mut cards: Vec<String> = STARTER_CARDS.iter().map(|c| c.to_string()).collect();
    for card in scaffold::template_cards(template) {
        if !cards.contains(&card) {
            cards.push(card);
        }
    }
    Ok(OnboardingPreview {
        setup_steps: setup_steps(answers),
        project_name: answers.name.clone(),
        cards,
        choice,
    })
}

/// Scaffolds from the foundation, writes the starter Context over the
/// template's placeholders, saves the technical level, and makes the
/// project's one first snapshot, "New game: <name>". Nothing is built and
/// no chat is started. If anything fails, everything written is removed
/// again, exactly like `scaffold::create_project`.
pub fn create_from_interview(parent: &Path, answers: &InterviewAnswers) -> Result<CreatedProject> {
    let choice = choose_template(answers)?;
    let (project, ()) = scaffold::create_project_with(
        parent,
        &answers.name,
        &choice.template_id,
        &format!("New game: {}", answers.name),
        |project| write_starter(project, answers),
    )?;
    Ok(CreatedProject {
        path: project.to_string_lossy().into_owned(),
    })
}

fn write_starter(project: &Path, answers: &InterviewAnswers) -> Result<()> {
    let context = project.join(".ibproject/context");
    for (card, text) in [
        (CONCEPT_CARD, concept_card(answers)),
        (STYLE_GUIDE_CARD, style_guide_card()),
    ] {
        let path = context.join(card);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        std::fs::write(&path, text).with_context(|| format!("writing {}", path.display()))?;
    }
    project_settings::save(
        project,
        &ProjectSettings {
            technical_level: answers.technical_level,
            ..ProjectSettings::default()
        },
    )
}

// --- Card text ------------------------------------------------------------

/// A YAML double-quoted scalar. JSON string syntax is a subset of it, so
/// any title (quotes, colons, emoji) stays one valid value.
fn yaml_string(value: &str) -> String {
    serde_json::to_string(value).expect("a string always serializes")
}

fn front_matter(kind: &str, title: &str, status: &str) -> String {
    format!(
        "---\ntype: {kind}\ntitle: {}\nstatus: {status}\n---\n",
        yaml_string(title)
    )
}

/// Their answer, trimmed; `None` when they left it empty.
fn answer(value: &str) -> Option<&str> {
    Some(value.trim()).filter(|v| !v.is_empty())
}

fn section(out: &mut String, heading: &str, body: &str) {
    out.push_str(&format!("\n## {heading}\n\n{body}\n"));
}

fn concept_card(answers: &InterviewAnswers) -> String {
    let mut card = front_matter("concept", &answers.name, "starting point");
    card.push_str(&format!("\n# {}\n", answers.name));
    match answer(&answers.idea) {
        Some(idea) => section(&mut card, "Pitch", idea),
        None => section(&mut card, "Pitch", "> Not written yet: the game in one or two sentences."),
    }
    if let Some(references) = answer(&answers.references) {
        section(&mut card, "Games it's like", references);
    }
    section(&mut card, "Pillars", "> Not decided yet: the three or so things the game must get right.");
    section(&mut card, "Core loop", "> Not decided yet: what the player does again and again, and why it is fun.");
    section(&mut card, "Scope", "> Not decided yet: length, number of levels or areas, platforms, team.");
    card.push_str(
        "\n> Only the pitch and the games it is like come from the questions asked when the \
         game was created. Everything else is decided together with the AI, one step at a \
         time, starting with the pre-production tasks.\n",
    );
    card
}

fn style_guide_card() -> String {
    let mut card = front_matter("style-guide", "Style guide", "starting point");
    card.push_str("\n# Style guide\n");
    section(&mut card, "Look", "> Not decided yet: art style, palette, references.");
    section(&mut card, "Sound", "> Not decided yet: music and sound mood.");
    section(&mut card, "Tone", "> Not decided yet: how the game should feel to play.");
    card
}

/// What creating the game does, for the review screen (first person).
fn setup_steps(answers: &InterviewAnswers) -> Vec<String> {
    let dimension = if answers.genre.trim() == GENRE_3D { "3D" } else { "2D" };
    vec![
        format!("Set up “{}” as an organised {dimension} project, ready to grow", answers.name),
        "Add the systems every big game needs: a menu, game flow, scenes, saving and settings".to_string(),
        "Write what you told me into the concept and style notes, and leave the rest open".to_string(),
        "Add the first planning tasks, to work through together".to_string(),
        "Explain each system in your Documents, at the level you chose".to_string(),
        "Build nothing else: what the game is, and what comes first, is yours to decide with me".to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::snapshot;

    fn answers(name: &str, genre: &str) -> InterviewAnswers {
        InterviewAnswers {
            idea: "A fox who jumps between rooftops at night".into(),
            genre: genre.into(),
            references: "Celeste, A Short Hike".into(),
            technical_level: TechnicalLevel::Guided,
            name: name.into(),
        }
    }

    #[test]
    fn the_foundation_follows_the_dimension_and_nothing_else_is_accepted() {
        assert_eq!(choose_template(&answers("A", "2d")).unwrap().template_id, FOUNDATION_2D_ID);
        assert_eq!(choose_template(&answers("A", "3d")).unwrap().template_id, FOUNDATION_3D_ID);
        assert!(choose_template(&answers("A", "")).is_err());
        assert!(choose_template(&answers("A", "platformer-2d")).is_err());
    }

    #[test]
    fn preview_lists_the_cards_and_the_setup_without_touching_the_disk() {
        let shown = preview(&answers("Rooftop Fox", "2d")).unwrap();
        assert_eq!(shown.project_name, "Rooftop Fox");
        assert_eq!(&shown.cards[..2], STARTER_CARDS.map(String::from).as_slice());
        for expected in ["systems/overview.md", "systems/saving.md", "tasks/define-the-pillars.md"] {
            assert!(shown.cards.iter().any(|c| c == expected), "{expected} in {:?}", shown.cards);
        }
        assert!(shown.setup_steps.iter().any(|s| s.contains("Build nothing else")));
        assert!(preview(&answers("bad/name", "2d")).is_err());
        assert!(preview(&answers("Ok", "")).is_err());
    }

    #[test]
    fn creating_a_game_builds_nothing_and_starts_no_chat() {
        let parent = tempfile::tempdir().unwrap();
        let created = create_from_interview(parent.path(), &answers("Rooftop Fox", "3d")).unwrap();
        let project = Path::new(&created.path);

        // The foundation, not a playable template.
        for file in ["core/events.gd", "core/save_system.gd", "levels/sandbox.tscn", "ui/main_menu.tscn", "AGENTS.md"] {
            assert!(project.join(file).is_file(), "{file}");
        }
        let agents = fs::read_to_string(project.join("AGENTS.md")).unwrap();
        assert!(agents.contains("Rooftop Fox") && !agents.contains("{{PROJECT_NAME}}"));
        // No chat thread (nothing to send), one snapshot, and the settings they chose.
        assert!(!project.join(".ibproject/chat").exists() || fs::read_dir(project.join(".ibproject/chat")).unwrap().next().is_none());
        assert_eq!(snapshot::list_snapshots(project, 10).unwrap().len(), 1);
        assert_eq!(snapshot::list_snapshots(project, 10).unwrap()[0].title, "New game: Rooftop Fox");
        assert_eq!(project_settings::load(project).unwrap().technical_level, TechnicalLevel::Guided);

        // Their words are in the concept; the rest says it is open.
        let concept = fs::read_to_string(project.join(".ibproject/context/concept.md")).unwrap();
        assert!(concept.contains("A fox who jumps between rooftops at night"));
        assert!(concept.contains("Celeste, A Short Hike"));
        assert!(concept.contains("Not decided yet"));
        // The systems are explained, and the planning tasks are waiting.
        assert!(project.join(".ibproject/context/systems/events.md").is_file());
        assert!(project.join(".ibproject/context/tasks/define-the-core-loop.md").is_file());
        // Not a playable template.
        assert!(!project.join("scenes/player.tscn").exists());
    }

    #[test]
    fn an_empty_idea_is_left_out_not_invented() {
        let parent = tempfile::tempdir().unwrap();
        let mut a = answers("Blank Slate", "2d");
        a.idea = "  ".into();
        a.references = String::new();
        let created = create_from_interview(parent.path(), &a).unwrap();
        let concept = fs::read_to_string(Path::new(&created.path).join(".ibproject/context/concept.md")).unwrap();
        assert!(concept.contains("Not written yet"));
        assert!(!concept.contains("Games it's like"));
    }

    #[test]
    fn a_failed_creation_leaves_nothing_behind() {
        let parent = tempfile::tempdir().unwrap();
        assert!(create_from_interview(parent.path(), &answers("Oops", "")).is_err());
        assert!(!parent.path().join("Oops").exists());
        // A name that already has files in it is refused without touching them.
        fs::create_dir(parent.path().join("Taken")).unwrap();
        fs::write(parent.path().join("Taken/keep.txt"), "x").unwrap();
        assert!(create_from_interview(parent.path(), &answers("Taken", "2d")).is_err());
        assert!(parent.path().join("Taken/keep.txt").is_file());
    }
}
