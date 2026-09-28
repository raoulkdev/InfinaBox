//! The first-run interview (spec §6.2): turns the person's answers into a
//! template choice with a plain reason, a starter Context, a new project,
//! and the message for its first build.
//!
//! Everything here is deterministic (Phase B "Decisions"): the template is
//! the person's own pick, or keyword matching over their idea; the starter
//! cards are built from their own words plus the template's
//! `template.json`, and nothing is invented to fill a gap — an answer left
//! empty is simply left out. The AI refines the cards during the first
//! build.
//!
//! A new game's history starts as **one** snapshot, "New game: <name>":
//! the template, the starter cards and the "First build" chat thread are
//! all written before the project's first commit
//! (`scaffold::create_project_with`), so there is no "New project"
//! snapshot holding the template's placeholder concept for someone to go
//! back to by mistake.

use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::chat_store;
use crate::scaffold::{self, BLANK_TEMPLATE_ID, TemplateInfo};

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

/// The template picked when nothing in the idea points anywhere: it is the
/// most flexible of the three, and the first build reshapes it anyway.
const FALLBACK_TEMPLATE_ID: &str = "platformer-2d";

/// The `genre` answer for "Something else".
pub const GENRE_OTHER: &str = "other";

/// Title of the chat thread the first build runs in.
pub const FIRST_BUILD_THREAD_TITLE: &str = "First build";

/// The starter cards InfinaBox writes, relative to `.ibproject/context/`.
/// `concept.md` replaces the template's own; its mechanic cards stay.
const CONCEPT_CARD: &str = "concept.md";
const STYLE_GUIDE_CARD: &str = "style-guide.md";
const FIRST_PLAYABLE_CARD: &str = "tasks/first-playable.md";
const STARTER_CARDS: [&str; 3] = [CONCEPT_CARD, STYLE_GUIDE_CARD, FIRST_PLAYABLE_CARD];

/// The person's genre pick when it names a template (including "Start from
/// scratch"); otherwise keyword matching over their idea and their own
/// genre description. Keyword matching never picks the blank template.
pub fn choose_template(answers: &InterviewAnswers, templates: &[TemplateInfo]) -> TemplateChoice {
    let genre = answers.genre.trim();
    if let Some(picked) = templates.iter().find(|t| t.id == genre) {
        return TemplateChoice {
            template_id: picked.id.clone(),
            reason: format!("You picked “{}”.", picked.name),
        };
    }

    let text = format!(
        "{}\n{}",
        answers.idea,
        answers.genre_other.as_deref().unwrap_or_default()
    );
    let mut best: Option<(&TemplateInfo, Vec<String>)> = None;
    for template in templates.iter().filter(|t| t.id != BLANK_TEMPLATE_ID) {
        let matched = matched_keywords(&text, &template.keywords);
        // Most distinct matches wins; a tie goes to the template offered
        // first.
        if !matched.is_empty() && best.as_ref().is_none_or(|(_, b)| matched.len() > b.len()) {
            best = Some((template, matched));
        }
    }
    if let Some((template, matched)) = best {
        let quoted: Vec<String> = matched.iter().map(|w| format!("“{w}”")).collect();
        return TemplateChoice {
            template_id: template.id.clone(),
            reason: format!(
                "Your idea mentions {}, so we start from “{}”.",
                and_list(&quoted),
                template.name
            ),
        };
    }

    let fallback = templates
        .iter()
        .find(|t| t.id == FALLBACK_TEMPLATE_ID)
        .or_else(|| templates.iter().find(|t| t.id != BLANK_TEMPLATE_ID));
    match fallback {
        Some(template) => TemplateChoice {
            template_id: template.id.clone(),
            reason: format!(
                "Nothing in your idea points at one kind of game yet, so we start from \
                 “{}”: it's the most flexible starting point, and the AI will reshape it \
                 around your idea.",
                template.name
            ),
        },
        // Only reachable if no genre template exists at all.
        None => TemplateChoice {
            template_id: BLANK_TEMPLATE_ID.to_string(),
            reason: "We start from scratch, and the AI builds your idea from there.".to_string(),
        },
    }
}

/// The template's keywords found in `text` as whole words, ignoring case,
/// in the template's order, without repeats. A plain `-s`/`-es` ending
/// counts ("jumps" is "jump"); other endings don't ("jumping" isn't).
fn matched_keywords(text: &str, keywords: &[String]) -> Vec<String> {
    let mut matched: Vec<String> = Vec::new();
    for keyword in keywords {
        let keyword = keyword.trim().to_lowercase();
        if keyword.is_empty() || matched.contains(&keyword) {
            continue;
        }
        // `\b` only means "word edge" next to a word character; keywords
        // are words (maybe hyphenated), but don't trust that blindly.
        let is_word = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
        let start = if is_word(keyword.chars().next()) {
            r"\b"
        } else {
            ""
        };
        let end = if is_word(keyword.chars().last()) {
            r"(?:s|es)?\b"
        } else {
            ""
        };
        let pattern = format!("(?i){start}{}{end}", regex::escape(&keyword));
        if regex::Regex::new(&pattern).is_ok_and(|re| re.is_match(text)) {
            matched.push(keyword);
        }
    }
    matched
}

/// What the review screen shows before anything is created. Checks the
/// name the same way creating the project will; never touches the disk.
pub fn preview(answers: &InterviewAnswers) -> Result<OnboardingPreview> {
    scaffold::validate_name(&answers.name)?;
    let choice = choose_template(answers, &scaffold::list_templates());
    let (template, info) = scaffold::find_template(&choice.template_id)?;

    let mut cards: Vec<String> = STARTER_CARDS.iter().map(|c| c.to_string()).collect();
    for card in scaffold::template_cards(template) {
        if !cards.contains(&card) {
            cards.push(card);
        }
    }
    Ok(OnboardingPreview {
        first_build_steps: first_build_steps(answers, &info),
        project_name: answers.name.clone(),
        cards,
        choice,
    })
}

/// Scaffolds from the chosen template, writes the starter Context over the
/// template's cards, creates the "First build" chat thread (for
/// `provider`), and makes the project's one first snapshot, "New game:
/// <name>". Returns the first build message, which the app sends as that
/// thread's first message. If anything fails, everything written is
/// removed again, exactly like `scaffold::create_project`.
pub fn create_from_interview(
    parent: &Path,
    answers: &InterviewAnswers,
    template_id: &str,
    provider: &str,
) -> Result<CreatedProject> {
    // Refuses an unknown template before anything is written.
    let (template, info) = scaffold::find_template(template_id)?;
    let choice = choose_template(answers, &scaffold::list_templates());
    let reason = if choice.template_id == template_id {
        choice.reason
    } else {
        // They changed the suggestion on the review screen.
        format!("You picked “{}”.", info.name)
    };

    let (project, thread_id) = scaffold::create_project_with(
        parent,
        &answers.name,
        template_id,
        &format!("New game: {}", answers.name),
        |project| write_starter(project, answers, &info, &reason, provider),
    )?;

    Ok(CreatedProject {
        path: project.to_string_lossy().into_owned(),
        thread_id,
        first_build_message: first_build_message(answers, &info, &mechanic_cards(template)),
    })
}

/// Writes the starter cards over the freshly copied template and creates
/// the "First build" thread; returns the thread's id.
fn write_starter(
    project: &Path,
    answers: &InterviewAnswers,
    info: &TemplateInfo,
    reason: &str,
    provider: &str,
) -> Result<String> {
    let context = project.join(".ibproject/context");
    for (card, text) in [
        (CONCEPT_CARD, concept_card(answers, info, reason)),
        (STYLE_GUIDE_CARD, style_guide_card(answers)),
        (FIRST_PLAYABLE_CARD, first_playable_card(answers, info)),
    ] {
        let path = context.join(card);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        std::fs::write(&path, text).with_context(|| format!("writing {}", path.display()))?;
    }
    let thread = chat_store::create_thread(project, FIRST_BUILD_THREAD_TITLE, provider)
        .context("creating the first build's chat")?;
    Ok(thread.id)
}

/// The template's own cards that stay as they are (everything but the
/// cards InfinaBox writes).
fn mechanic_cards(template: &include_dir::Dir<'_>) -> Vec<String> {
    scaffold::template_cards(template)
        .into_iter()
        .filter(|c| !STARTER_CARDS.contains(&c.as_str()))
        .collect()
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

/// Their feel words, trimmed, without blanks or repeats.
fn feel_words(answers: &InterviewAnswers) -> Vec<String> {
    let mut words: Vec<String> = Vec::new();
    for word in answers.feel.iter().map(|w| w.trim()) {
        if !word.is_empty() && !words.iter().any(|w| w.eq_ignore_ascii_case(word)) {
            words.push(word.to_string());
        }
    }
    words
}

/// "a", "a and b", "a, b and c".
fn and_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// `text` on one line (for checklist items and sentences).
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn section(out: &mut String, heading: &str, body: &str) {
    out.push_str(&format!("\n## {heading}\n\n{body}\n"));
}

fn bullets(items: &[String]) -> String {
    items
        .iter()
        .map(|i| format!("- {i}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn concept_card(answers: &InterviewAnswers, template: &TemplateInfo, reason: &str) -> String {
    let mut card = front_matter("concept", &answers.name, "draft");
    card.push_str(&format!("\n# {}\n", answers.name));
    if let Some(idea) = answer(&answers.idea) {
        section(&mut card, "Pitch", idea);
    }
    let feel = feel_words(answers);
    if !feel.is_empty() {
        section(&mut card, "How it should feel", &bullets(&feel));
    }
    if let Some(look) = answer(&answers.look) {
        section(&mut card, "Look", look);
    }
    if let Some(references) = answer(&answers.references) {
        section(&mut card, "Games it's like", references);
    }
    if let Some(length) = answer(&answers.session_length) {
        section(&mut card, "Session length", length);
    }
    section(
        &mut card,
        "Starting point",
        &format!(
            "Started from the “{}” template: {}\n\nWhy this template: {reason}",
            template.name, template.description
        ),
    );
    card
}

fn style_guide_card(answers: &InterviewAnswers) -> String {
    let mut card = front_matter("style-guide", "Style guide", "draft");
    card.push_str("\n# Style guide\n");
    if let Some(look) = answer(&answers.look) {
        section(&mut card, "Look", look);
    }
    let feel = feel_words(answers);
    if !feel.is_empty() {
        section(&mut card, "Feel", &bullets(&feel));
    }
    card.push_str(
        "\n> A first draft from the answers given when this game was created. The AI \
         refines it while building: the colors, shapes, sizes and sounds it settles on \
         belong here.\n",
    );
    card
}

fn first_playable_card(answers: &InterviewAnswers, template: &TemplateInfo) -> String {
    let mut card = front_matter("task", "First playable", "todo");
    card.push_str(&format!(
        "\n# First playable\n\nThe first milestone: one playable level of {}, reshaped \
         from the “{}” template.\n\n",
        answers.name, template.name
    ));
    let mut items: Vec<String> = template.features.iter().map(|f| one_line(f)).collect();
    if let Some(idea) = answer(&answers.idea) {
        items.push(format!("Make it feel like “{}”", one_line(idea)));
    }
    let feel = feel_words(answers);
    if !feel.is_empty() {
        items.push(format!("Feels {}", and_list(&feel).to_lowercase()));
    }
    items.push("Runs with no errors".to_string());
    for item in items {
        card.push_str(&format!("- [ ] {item}\n"));
    }
    section(&mut card, "How to play", &template.controls);
    card
}

// --- The first build ------------------------------------------------------

/// The instructions for the first build turn, sent as the "First build"
/// thread's first message (origin `first_build`).
fn first_build_message(
    answers: &InterviewAnswers,
    template: &TemplateInfo,
    mechanic_cards: &[String],
) -> String {
    // Features often contain "and" themselves, so they're separated by
    // semicolons rather than joined into one "a, b and c" list.
    let features: Vec<String> = template
        .features
        .iter()
        .map(|f| one_line(f).to_lowercase())
        .collect();
    let cards: Vec<String> = STARTER_CARDS.iter().map(|c| format!("`{c}`")).collect();
    let mechanics = if mechanic_cards.is_empty() {
        String::new()
    } else {
        let listed: Vec<String> = mechanic_cards.iter().map(|c| format!("`{c}`")).collect();
        format!(
            ", and the template's mechanic cards ({})",
            and_list(&listed)
        )
    };

    format!(
        "This is the first build of a new game, “{name}”. The project is a copy of the \
         “{template}” template, which already works: {features}.\n\n\
         Start by reading the Context cards in `.ibproject/context/`: {cards} (written from \
         the person's answers when they created the game){mechanics}.\n\n\
         Then customize the template toward the concept: the player, the tuning of the core \
         mechanic so it matches how the game should feel, the colors so they match the look, \
         and the in-game title. Keep it to one playable level.\n\n\
         Run the game and fix any errors until it runs cleanly. Then update the cards: the \
         status of `{task}`, and which scenes and scripts implement what.\n\n\
         Finish with a short, friendly explanation of what you built and how to play \
         ({controls}).",
        name = answers.name,
        template = template.name,
        features = features.join("; "),
        cards = and_list(&cards),
        task = FIRST_PLAYABLE_CARD,
        controls = one_line(&template.controls),
    )
}

/// The first build's plan in plain words for the review screen, first
/// person ("What I'll build first").
fn first_build_steps(answers: &InterviewAnswers, template: &TemplateInfo) -> Vec<String> {
    let feel = feel_words(answers);
    vec![
        "Read your game's notes: the concept, the style guide and the first goal".to_string(),
        format!(
            "Turn “{}” into {}: the player, the colors and the title",
            template.name, answers.name
        ),
        if feel.is_empty() {
            "Tune how it plays to match your idea".to_string()
        } else {
            format!(
                "Tune how it plays so it feels {}",
                and_list(&feel).to_lowercase()
            )
        },
        "Keep it to one level, play it, and fix anything that breaks".to_string(),
        "Tell you what I built and how to play".to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;

    use super::*;
    use crate::snapshot;

    fn answers(name: &str) -> InterviewAnswers {
        InterviewAnswers {
            idea: "A fox who jumps between rooftops at night, collecting lost stars".into(),
            genre: "platformer-2d".into(),
            genre_other: None,
            feel: vec!["Cozy".into(), "fast".into(), " ".into(), "cozy".into()],
            look: "Soft pastel pixel art".into(),
            references: "Celeste, A Short Hike".into(),
            session_length: "10 minutes".into(),
            name: name.into(),
        }
    }

    fn template(id: &str, keywords: &[&str]) -> TemplateInfo {
        TemplateInfo {
            id: id.into(),
            name: format!("Name of {id}"),
            description: "d".into(),
            dimension: "2d".into(),
            controls: "c".into(),
            features: vec!["f".into()],
            keywords: keywords.iter().map(|k| k.to_string()).collect(),
        }
    }

    fn fake_templates() -> Vec<TemplateInfo> {
        vec![
            template("platformer-2d", &["jump", "levels", "side-scroller"]),
            template("topdown-2d", &["explore", "top-down", "cozy"]),
            template("shooter-2d", &["shoot", "waves", "zombies"]),
            template(BLANK_TEMPLATE_ID, &["anything", "jump"]),
        ]
    }

    fn other(idea: &str, genre_other: Option<&str>) -> InterviewAnswers {
        InterviewAnswers {
            idea: idea.into(),
            genre: GENRE_OTHER.into(),
            genre_other: genre_other.map(str::to_string),
            ..Default::default()
        }
    }

    /// The front-matter of a card as key → value (quoted values unquoted
    /// the way YAML reads them), and the body after it.
    fn parse_card(text: &str) -> (BTreeMap<String, String>, String) {
        let rest = text
            .strip_prefix("---\n")
            .expect("starts with front-matter");
        let (head, body) = rest.split_once("\n---\n").expect("front-matter closes");
        let mut fields = BTreeMap::new();
        for line in head.lines() {
            let (key, value) = line
                .split_once(": ")
                .unwrap_or_else(|| panic!("bad line {line:?}"));
            let value = if value.starts_with('"') {
                serde_json::from_str::<String>(value).expect("a valid quoted scalar")
            } else {
                value.to_string()
            };
            assert!(
                fields.insert(key.to_string(), value).is_none(),
                "repeated {key}"
            );
        }
        (fields, body.to_string())
    }

    #[test]
    fn onboarding_uses_the_genre_the_person_picked() {
        let templates = fake_templates();
        for id in [
            "platformer-2d",
            "topdown-2d",
            "shooter-2d",
            BLANK_TEMPLATE_ID,
        ] {
            let mut a = other("I want to shoot zombies", None);
            a.genre = id.into();
            let choice = choose_template(&a, &templates);
            assert_eq!(choice.template_id, id);
            assert_eq!(choice.reason, format!("You picked “Name of {id}”."));
        }
    }

    #[test]
    fn onboarding_matches_keywords_as_whole_words_ignoring_case() {
        let templates = fake_templates();

        let choice = choose_template(&other("Shoot the ZOMBIES in waves!", None), &templates);
        assert_eq!(choice.template_id, "shooter-2d");
        assert_eq!(
            choice.reason,
            "Your idea mentions “shoot”, “waves” and “zombies”, so we start from “Name of shooter-2d”."
        );

        // The description they typed for "Something else" counts too, and
        // hyphenated keywords match.
        let choice = choose_template(&other("a little fox", Some("Top-Down, cozy")), &templates);
        assert_eq!(choice.template_id, "topdown-2d");
        assert!(
            choice.reason.contains("“top-down” and “cozy”"),
            "{}",
            choice.reason
        );

        // "jumping" and "shootout" aren't "jump" or "shoot"; "jump-and-run"
        // contains "jump".
        let choice = choose_template(&other("jumping into a shootout", None), &templates);
        assert_eq!(choice.template_id, "platformer-2d");
        assert!(
            choice.reason.starts_with("Nothing in your idea"),
            "{}",
            choice.reason
        );
        let choice = choose_template(&other("a jump-and-run", None), &templates);
        assert_eq!(choice.template_id, "platformer-2d");
        assert!(choice.reason.contains("“jump”"), "{}", choice.reason);
        // A plain plural or verb "-s" still counts.
        let choice = choose_template(&other("She jumps; it shoots", None), &templates);
        assert!(choice.reason.contains("“jump”"), "{}", choice.reason);
        let choice = choose_template(&other("it shoots zombies and explores", None), &templates);
        assert_eq!(choice.template_id, "shooter-2d", "{}", choice.reason);
    }

    #[test]
    fn onboarding_most_matches_wins_and_ties_go_to_the_first_offered() {
        let templates = fake_templates();
        let choice = choose_template(
            &other("jump to explore cozy top-down rooms", None),
            &templates,
        );
        assert_eq!(choice.template_id, "topdown-2d");
        let choice = choose_template(&other("jump and shoot", None), &templates);
        assert_eq!(choice.template_id, "platformer-2d");
    }

    #[test]
    fn onboarding_never_picks_blank_by_itself() {
        let templates = fake_templates();
        // "anything" is only a blank-2d keyword.
        let choice = choose_template(&other("anything at all", None), &templates);
        assert_eq!(choice.template_id, "platformer-2d");
        assert!(choice.reason.contains("most flexible"), "{}", choice.reason);
        assert!(choice.reason.contains("reshape"), "{}", choice.reason);

        // An unknown genre id is treated like "Something else".
        let mut a = other("", None);
        a.genre = "puzzle-3d".into();
        assert_eq!(choose_template(&a, &templates).template_id, "platformer-2d");

        // Without a platformer, the first genre template.
        let without: Vec<TemplateInfo> = templates[1..].to_vec();
        assert_eq!(choose_template(&a, &without).template_id, "topdown-2d");
    }

    /// Against the real bundled templates: each template's own keywords
    /// lead back to it (the keyword lists themselves are the templates').
    #[test]
    fn onboarding_real_templates_are_found_by_their_own_keywords() {
        let templates = scaffold::list_templates();
        for t in templates.iter().filter(|t| t.id != BLANK_TEMPLATE_ID) {
            let idea = t.keywords.join(" ");
            assert_eq!(
                choose_template(&other(&idea, None), &templates).template_id,
                t.id
            );
        }
    }

    /// `preview` takes no folder, so there is nothing on disk for it to
    /// touch; this checks what it reports.
    #[test]
    fn onboarding_preview_shows_the_plan() {
        let mut a = answers("Rooftop Fox");
        a.genre = GENRE_OTHER.into();
        let shown = preview(&a).unwrap();

        assert_eq!(shown.project_name, "Rooftop Fox");
        assert_eq!(
            shown.choice,
            choose_template(&a, &scaffold::list_templates())
        );
        assert!(!shown.choice.reason.is_empty());
        assert_eq!(
            &shown.cards[..3],
            STARTER_CARDS.map(String::from).as_slice()
        );
        let (template, _) = scaffold::find_template(&shown.choice.template_id).unwrap();
        for card in scaffold::template_cards(template) {
            assert!(
                shown.cards.contains(&card),
                "{card} missing from {:?}",
                shown.cards
            );
        }
        let mut unique = shown.cards.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), shown.cards.len());

        assert!((3..=5).contains(&shown.first_build_steps.len()));
        assert!(
            shown
                .first_build_steps
                .iter()
                .any(|s| s.contains("feels cozy and fast"))
        );

        a.name = "a/b".into();
        assert!(preview(&a).is_err());
    }

    #[test]
    fn onboarding_creates_a_project_with_a_starter_context() {
        let tmp = tempfile::tempdir().unwrap();
        for info in scaffold::list_templates() {
            let name = format!("Fox {}", info.id);
            let mut a = answers(&name);
            a.genre = info.id.clone();
            let created = create_from_interview(tmp.path(), &a, &info.id, "claude-code")
                .unwrap_or_else(|e| panic!("{}: {e:#}", info.id));
            let project = tmp.path().join(&name);
            assert_eq!(created.path, project.to_string_lossy());
            assert!(project.join(".ibproject/.ibx").is_file());
            let context = project.join(".ibproject/context");

            let (fields, body) =
                parse_card(&fs::read_to_string(context.join(CONCEPT_CARD)).unwrap());
            assert_eq!(fields["type"], "concept");
            assert_eq!(fields["title"], name);
            assert_eq!(fields["status"], "draft");
            for expected in [
                a.idea.as_str(),
                "- Cozy\n- fast\n",
                "Soft pastel pixel art",
                "Celeste, A Short Hike",
                "10 minutes",
                &format!("“{}” template", info.name),
                &format!("You picked “{}”.", info.name),
            ] {
                assert!(
                    body.contains(expected),
                    "{}: concept lacks {expected:?}:\n{body}",
                    info.id
                );
            }
            assert!(!body.contains("{{"), "{body}");

            let (fields, body) =
                parse_card(&fs::read_to_string(context.join(STYLE_GUIDE_CARD)).unwrap());
            assert_eq!(fields["type"], "style-guide");
            assert!(body.contains("Soft pastel pixel art") && body.contains("- Cozy"));
            assert!(body.contains("The AI refines it"));

            let (fields, body) =
                parse_card(&fs::read_to_string(context.join(FIRST_PLAYABLE_CARD)).unwrap());
            assert_eq!(fields["type"], "task");
            assert_eq!(fields["status"], "todo");
            for feature in &info.features {
                assert!(
                    body.contains(&format!("- [ ] {feature}\n")),
                    "{}: {body}",
                    info.id
                );
            }
            assert!(body.contains(&format!("- [ ] Make it feel like “{}”\n", a.idea)));
            assert!(body.contains(&info.controls));

            // The template's own cards are all still there.
            let (template, _) = scaffold::find_template(&info.id).unwrap();
            for card in scaffold::template_cards(template) {
                assert!(context.join(&card).is_file(), "{}: lost {card}", info.id);
            }

            // The thread exists and is part of the one first snapshot.
            let threads = chat_store::list_threads(&project).unwrap();
            assert_eq!(threads.len(), 1);
            assert_eq!(threads[0].id, created.thread_id);
            assert_eq!(threads[0].title, FIRST_BUILD_THREAD_TITLE);
            assert_eq!(threads[0].provider, "claude-code");
            let snapshots = snapshot::list_snapshots(&project, 10).unwrap();
            assert_eq!(snapshots.len(), 1, "{snapshots:?}");
            assert_eq!(snapshots[0].title, format!("New game: {name}"));
            let repo = git2::Repository::open(&project).unwrap();
            assert!(
                repo.statuses(None).unwrap().is_empty(),
                "{}: uncommitted files",
                info.id
            );
            let tree = repo.head().unwrap().peel_to_tree().unwrap();
            let thread_file = format!(".ibproject/chat/{}.jsonl", created.thread_id);
            assert!(tree.get_path(Path::new(&thread_file)).is_ok());
            assert!(tree.get_path(Path::new(".ibproject/.ibx")).is_ok());

            // The first build message covers the plan.
            let message = &created.first_build_message;
            for expected in [
                "first build",
                &name,
                "concept.md",
                "style-guide.md",
                "tasks/first-playable.md",
                "one playable level",
                "fix any errors",
                "update the cards",
                &one_line(&info.controls),
            ] {
                assert!(
                    message.contains(expected),
                    "{}: message lacks {expected:?}",
                    info.id
                );
            }
            for card in mechanic_cards(template) {
                assert!(message.contains(&card), "{}: message lacks {card}", info.id);
            }
        }
    }

    /// Empty answers are left out, not filled in.
    #[test]
    fn onboarding_leaves_out_empty_answers() {
        let tmp = tempfile::tempdir().unwrap();
        let a = InterviewAnswers {
            genre: BLANK_TEMPLATE_ID.into(),
            name: "Bare".into(),
            feel: vec!["  ".into()],
            ..Default::default()
        };
        create_from_interview(tmp.path(), &a, BLANK_TEMPLATE_ID, "codex").unwrap();
        let context = tmp.path().join("Bare/.ibproject/context");

        let (_, concept) = parse_card(&fs::read_to_string(context.join(CONCEPT_CARD)).unwrap());
        for heading in [
            "Pitch",
            "How it should feel",
            "Look",
            "Games it's like",
            "Session length",
        ] {
            assert!(
                !concept.contains(&format!("## {heading}")),
                "{heading}:\n{concept}"
            );
        }
        assert!(concept.contains("## Starting point"));
        let (_, style) = parse_card(&fs::read_to_string(context.join(STYLE_GUIDE_CARD)).unwrap());
        assert!(!style.contains("## "), "{style}");
        let (_, task) = parse_card(&fs::read_to_string(context.join(FIRST_PLAYABLE_CARD)).unwrap());
        assert!(
            !task.contains("Make it feel like") && !task.contains("Feels"),
            "{task}"
        );
    }

    #[test]
    fn onboarding_quotes_odd_names_in_front_matter() {
        let card = concept_card(
            &answers("Say \"hi\": a #1 game"),
            &template("platformer-2d", &[]),
            "r",
        );
        let (fields, _) = parse_card(&card);
        assert_eq!(fields["title"], "Say \"hi\": a #1 game");
    }

    #[test]
    fn onboarding_rolls_back_a_failed_creation() {
        let tmp = tempfile::tempdir().unwrap();
        let a = answers("Rollback");

        // Refused before anything is written: an unknown template, a bad name.
        assert!(create_from_interview(tmp.path(), &a, "nope", "claude-code").is_err());
        let mut bad = a.clone();
        bad.name = "trailing.".into();
        assert!(create_from_interview(tmp.path(), &bad, "platformer-2d", "claude-code").is_err());
        assert_eq!(fs::read_dir(tmp.path()).unwrap().count(), 0);

        // A folder that exists and isn't empty is left exactly as it was.
        let taken = tmp.path().join("Rollback");
        fs::create_dir(&taken).unwrap();
        fs::write(taken.join("notes.txt"), "mine").unwrap();
        assert!(create_from_interview(tmp.path(), &a, "platformer-2d", "claude-code").is_err());
        assert_eq!(fs::read_dir(&taken).unwrap().count(), 1);

        // A step failing after the starter cards and thread were written
        // (the snapshot is the only step left, and it can't be made to fail
        // on demand, so a failing step stands in for it) removes it all.
        let (_, info) = scaffold::find_template("platformer-2d").unwrap();
        let err = scaffold::create_project_with(
            tmp.path(),
            "Half Made",
            "platformer-2d",
            "t",
            |p| -> Result<()> {
                write_starter(p, &a, &info, "r", "claude-code")?;
                assert!(
                    p.join(".ibproject/context")
                        .join(FIRST_PLAYABLE_CARD)
                        .is_file()
                );
                anyhow::bail!("boom")
            },
        )
        .unwrap_err();
        assert_eq!(err.to_string(), "boom");
        assert!(!tmp.path().join("Half Made").exists());
    }
}
