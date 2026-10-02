//! Skills: saved workflows for a game ("how we add an enemy here").
//!
//! A skill is an ordinary Context card inside `skills/` (so it is edited in
//! Documents, found by search, and listed to the AI like any card). The
//! file name is its command: `skills/add-an-enemy.md` is `/add-an-enemy`.
//! When a typed message starts with a skill's command, `expand` wraps the
//! skill's text around it so the AI follows the skill for that request.

use std::path::Path;

use anyhow::{Result, bail};
use serde::Serialize;

use crate::context_cards::{self, CardMeta, CardType};

/// The folder of skills, inside the context folder.
pub const SKILLS_FOLDER: &str = "skills";

/// A skill's text isn't sent beyond this much.
const MAX_SKILL_BYTES: usize = 24 * 1024;

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct SkillInfo {
    /// What follows the `/` to use it.
    pub slug: String,
    pub title: String,
    /// The first line of text under the title.
    pub description: String,
    /// The card's path in the context folder.
    pub path: String,
}

/// `skills/<slug>.md` for a slug, if it is a valid one (lowercase letters,
/// digits and single hyphens).
fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 60
        && !slug.starts_with('-')
        && !slug.ends_with('-')
        && !slug.contains("--")
        && slug.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// A command name for a skill title: "Add an enemy!" -> "add-an-enemy".
pub fn slugify(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.truncate(60);
    out.trim_end_matches('-').to_string()
}

fn description_of(body: &str) -> String {
    body.lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with('>'))
        .unwrap_or("")
        .chars()
        .take(160)
        .collect()
}

/// Every skill in the game, by name.
pub fn list(project: &Path) -> Result<Vec<SkillInfo>> {
    let mut skills = Vec::new();
    for summary in context_cards::list_cards(project)? {
        let Some(file) = summary.path.strip_prefix("skills/") else { continue };
        let Some(slug) = file.strip_suffix(".md") else { continue };
        if !valid_slug(slug) {
            continue;
        }
        let card = context_cards::read_card(project, &summary.path)?;
        skills.push(SkillInfo {
            slug: slug.to_string(),
            title: summary.title,
            description: description_of(&card.body),
            path: summary.path,
        });
    }
    skills.sort_by(|a, b| a.slug.cmp(&b.slug));
    Ok(skills)
}

/// Starts a skill from the title the person typed. Fails if one with that
/// command already exists.
pub fn create(project: &Path, name: &str) -> Result<SkillInfo> {
    let title = name.trim();
    let slug = slugify(title);
    if slug.is_empty() {
        bail!("Give the skill a name with a few letters or numbers in it.");
    }
    let path = format!("{SKILLS_FOLDER}/{slug}.md");
    if context_cards::read_card(project, &path).is_ok() {
        bail!("There's already a skill called /{slug}.");
    }
    let meta = CardMeta {
        card_type: Some(CardType::Other),
        title: Some(title.to_string()),
        ..Default::default()
    };
    let body = format!(
        "When to use this, in one sentence.\n\n## Steps\n\n1. First step.\n2. Second step.\n\n## Files and systems it touches\n\n- \n"
    );
    context_cards::write_card(project, &path, &meta, &body)?;
    Ok(SkillInfo {
        slug,
        title: title.to_string(),
        description: description_of(&body),
        path,
    })
}

/// If `message` starts with `/<skill>`, the message to send the AI: the
/// skill's text, then what the person added. `None` for any other message
/// (including a `/word` that isn't a skill).
pub fn expand(project: &Path, message: &str) -> Option<String> {
    let rest = message.trim_start().strip_prefix('/')?;
    let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    let slug = &rest[..end];
    if !valid_slug(slug) {
        return None;
    }
    let path = format!("{SKILLS_FOLDER}/{slug}.md");
    let card = context_cards::read_card(project, &path).ok()?;
    let title = context_cards::display_title(&card);
    let mut body = card.body.trim().to_string();
    if body.len() > MAX_SKILL_BYTES {
        let mut cut = MAX_SKILL_BYTES;
        while !body.is_char_boundary(cut) {
            cut -= 1;
        }
        body.truncate(cut);
    }
    let extra = rest[end..].trim();
    let ask = if extra.is_empty() {
        "(They gave no details: ask what you need to know before starting.)"
    } else {
        extra
    };
    Some(format!(
        "The person used their saved skill \"{title}\" (Context card {path}). Follow it for this request.\n\n\
         <skill>\n{body}\n</skill>\n\nTheir request: {ask}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn slugs_come_from_titles() {
        assert_eq!(slugify("Add an enemy!"), "add-an-enemy");
        assert_eq!(slugify("  New   level (2D) "), "new-level-2d");
        assert_eq!(slugify("!!!"), "");
    }

    #[test]
    fn a_skill_is_created_listed_and_expanded() {
        let dir = project();
        let made = create(dir.path(), "Add an enemy").unwrap();
        assert_eq!(made.slug, "add-an-enemy");
        assert_eq!(made.path, "skills/add-an-enemy.md");
        assert!(create(dir.path(), "add an enemy").is_err(), "no duplicates");
        assert!(create(dir.path(), "???").is_err());

        let listed = list(dir.path()).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].title, "Add an enemy");
        assert_eq!(listed[0].description, "When to use this, in one sentence.");

        let text = expand(dir.path(), "/add-an-enemy a slime that hops").unwrap();
        assert!(text.contains("saved skill \"Add an enemy\""), "{text}");
        assert!(text.contains("## Steps"), "{text}");
        assert!(text.ends_with("Their request: a slime that hops"), "{text}");

        let bare = expand(dir.path(), "/add-an-enemy").unwrap();
        assert!(bare.contains("ask what you need"), "{bare}");
    }

    #[test]
    fn messages_that_are_not_skills_are_left_alone() {
        let dir = project();
        create(dir.path(), "Add an enemy").unwrap();
        assert_eq!(expand(dir.path(), "make the player faster"), None);
        assert_eq!(expand(dir.path(), "/unknown thing"), None);
        assert_eq!(expand(dir.path(), "/../etc/passwd"), None);
        assert_eq!(expand(dir.path(), "/"), None);
    }

    #[test]
    fn only_cards_in_the_skills_folder_count() {
        let dir = project();
        let meta = CardMeta::default();
        context_cards::write_card(dir.path(), "notes/idea.md", &meta, "# Idea\nx\n").unwrap();
        assert!(list(dir.path()).unwrap().is_empty());
        assert_eq!(expand(dir.path(), "/idea"), None);
    }
}
