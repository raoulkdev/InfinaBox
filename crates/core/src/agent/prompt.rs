//! The instructions every runtime gives the agent: the Director prompt,
//! plus sections chosen by the turn's `TurnOptions` (plan policy, why the
//! message was sent, teach mode), plus the project's own `AGENTS.md`.
//! Shared so Claude Code and Codex get exactly the same instructions.
//!
//! Each section is a Markdown file in `prompts/`, so the wording can be read
//! and edited as text. Which ones a turn gets:
//!
//! | Turn | Plan section |
//! |---|---|
//! | typed by the person, `AlwaysPlan` | `plan-always.md` |
//! | typed by the person, `SmallChangesDirect` | `plan-small-changes.md` |
//! | the person approved a plan | `origin-plan-approval.md` (never re-plans) |
//! | an automatic error fix | `origin-auto-fix.md` (never plans) |
//! | the onboarding's first build | `origin-first-build.md` (never plans) |
//!
//! then the section for how technical the person wants things
//! (`level-guided.md`, `level-balanced.md` or `level-technical.md`), then,
//! for a specialist role (anything but the Director), that role's
//! section from `prompts/roles/<slug>.md`, then always `explain.md` ("what
//! I did and why"), then `teach.md` when "Teach me" is on, then the
//! project's `AGENTS.md`.
//!
//! This module also holds the limits of a `propose_plan` call, so the MCP
//! server that accepts one and the stream parsers that turn it into
//! `AgentEvent::PlanProposed` agree on what a valid plan is.

use std::path::Path;

use super::types::{MessageOrigin, PlanPolicy, Role, TechnicalLevel, TurnOptions};

/// The Director system prompt: who the agent is talking to and how it works.
pub const DIRECTOR_PROMPT: &str = include_str!("prompts/director.md");

const PLAN_ALWAYS: &str = include_str!("prompts/plan-always.md");
const PLAN_SMALL_CHANGES: &str = include_str!("prompts/plan-small-changes.md");
const ORIGIN_PLAN_APPROVAL: &str = include_str!("prompts/origin-plan-approval.md");
const ORIGIN_AUTO_FIX: &str = include_str!("prompts/origin-auto-fix.md");
const ORIGIN_FIRST_BUILD: &str = include_str!("prompts/origin-first-build.md");
const EXPLAIN: &str = include_str!("prompts/explain.md");
const TEACH: &str = include_str!("prompts/teach.md");
const LEVEL_GUIDED: &str = include_str!("prompts/level-guided.md");
const LEVEL_BALANCED: &str = include_str!("prompts/level-balanced.md");
const LEVEL_TECHNICAL: &str = include_str!("prompts/level-technical.md");

const ROLE_DESIGNER: &str = include_str!("prompts/roles/designer.md");
const ROLE_PROGRAMMER: &str = include_str!("prompts/roles/programmer.md");
const ROLE_ARTIST: &str = include_str!("prompts/roles/artist.md");
const ROLE_SOUND: &str = include_str!("prompts/roles/sound.md");
const ROLE_QA: &str = include_str!("prompts/roles/qa.md");
const ROLE_PRODUCER: &str = include_str!("prompts/roles/producer.md");
const ROLE_MARKETER: &str = include_str!("prompts/roles/marketer.md");

/// The section for a specialist role; the Director has none (its prompt is
/// the base prompt itself).
fn role_section(role: Role) -> Option<&'static str> {
    match role {
        Role::Director => None,
        Role::Designer => Some(ROLE_DESIGNER),
        Role::Programmer => Some(ROLE_PROGRAMMER),
        Role::Artist => Some(ROLE_ARTIST),
        Role::Sound => Some(ROLE_SOUND),
        Role::Qa => Some(ROLE_QA),
        Role::Producer => Some(ROLE_PRODUCER),
        Role::Marketer => Some(ROLE_MARKETER),
    }
}

/// The InfinaBox MCP tool the agent calls to show the person a plan
/// (`crates/mcp-server`). Runtimes turn a call to it into
/// `AgentEvent::PlanProposed`.
pub const PROPOSE_PLAN_TOOL: &str = "propose_plan";

/// Longest plan title `propose_plan` accepts, in characters.
pub const MAX_PLAN_TITLE_CHARS: usize = 80;
/// Most steps a plan may have (the prompt asks for 2–6; this leaves room).
pub const MAX_PLAN_STEPS: usize = 8;
/// Longest single step, in characters.
pub const MAX_PLAN_STEP_CHARS: usize = 200;

/// The project's agent instructions, appended after everything else.
pub const PROJECT_INSTRUCTIONS_FILE: &str = "AGENTS.md";
/// More than this much of `AGENTS.md` isn't appended.
pub const MAX_PROJECT_INSTRUCTIONS_BYTES: usize = 64 * 1024;

/// The full instructions for one turn. `agents_md` is the project's
/// `AGENTS.md`, already read (and size-capped) by the caller — see
/// `read_agents_md`.
pub fn system_prompt(options: &TurnOptions, agents_md: Option<&str>) -> String {
    let mut prompt = format!(
        "{}\n\n{}",
        DIRECTOR_PROMPT.trim_end(),
        turn_instructions(options)
    );
    if let Some(text) = agents_md.map(str::trim).filter(|t| !t.is_empty()) {
        prompt.push_str(&format!(
            "\n---\n\n# This project's {PROJECT_INSTRUCTIONS_FILE}\n\n{text}\n"
        ));
    }
    prompt
}

/// The part of the instructions that depends on the turn's options: the
/// plan section, "what I did and why", and the lesson when "Teach me" is
/// on (ends with a newline).
pub fn turn_instructions(options: &TurnOptions) -> String {
    let plan = match (options.origin, options.plan_policy) {
        (MessageOrigin::User, PlanPolicy::AlwaysPlan) => PLAN_ALWAYS,
        (MessageOrigin::User, PlanPolicy::SmallChangesDirect) => PLAN_SMALL_CHANGES,
        (MessageOrigin::PlanApproval, _) => ORIGIN_PLAN_APPROVAL,
        (MessageOrigin::AutoFix, _) => ORIGIN_AUTO_FIX,
        (MessageOrigin::FirstBuild, _) => ORIGIN_FIRST_BUILD,
    };
    let level = match options.technical_level {
        TechnicalLevel::Guided => LEVEL_GUIDED,
        TechnicalLevel::Balanced => LEVEL_BALANCED,
        TechnicalLevel::Technical => LEVEL_TECHNICAL,
    };
    let mut sections = vec![plan.trim_end(), level.trim_end()];
    if let Some(role) = role_section(options.role) {
        sections.push(role.trim_end());
    }
    sections.push(EXPLAIN.trim_end());
    if options.teach {
        sections.push(TEACH.trim_end());
    }
    let mut text = sections.join("\n\n");
    text.push('\n');
    text
}

/// The message sent to a CLI that resumes an earlier conversation. A
/// resumed conversation keeps the system prompt it started with (Claude
/// Code ignores `--append-system-prompt` with `--resume`, checked against
/// the real CLI), so this turn's own instructions — a plan first or not,
/// why the message was sent, "Teach me" — travel with the message instead,
/// marked as InfinaBox's and replacing the earlier ones.
pub fn message_with_turn_instructions(options: &TurnOptions, message: &str) -> String {
    format!(
        "<infinabox-instructions>\nInfinaBox's instructions for this message. They replace \
         any earlier ones about plans, explanations and lessons.\n\n{}</infinabox-instructions>\n\n{message}",
        turn_instructions(options)
    )
}

/// The project's `AGENTS.md`, cut to `MAX_PROJECT_INSTRUCTIONS_BYTES`, or
/// `None` if it's missing or unreadable. Read as text only: nothing in the
/// project gets to run.
pub fn read_agents_md(project: &Path) -> Option<String> {
    let bytes = std::fs::read(project.join(PROJECT_INSTRUCTIONS_FILE)).ok()?;
    let cut = &bytes[..bytes.len().min(MAX_PROJECT_INSTRUCTIONS_BYTES)];
    Some(String::from_utf8_lossy(cut).into_owned())
}

/// A plan as it's shown to the person: title and steps trimmed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidPlan {
    pub title: String,
    pub steps: Vec<String>,
}

/// Checks a `propose_plan` call's arguments against the limits above: a
/// non-empty title of at most `MAX_PLAN_TITLE_CHARS`, and 1 to
/// `MAX_PLAN_STEPS` non-empty steps of at most `MAX_PLAN_STEP_CHARS` each
/// (all measured after trimming). The error says what to fix, in words the
/// agent can act on.
pub fn validate_plan(title: &str, steps: &[String]) -> Result<ValidPlan, String> {
    let title = title.trim();
    if title.is_empty() {
        return Err("The plan needs a short title.".into());
    }
    if title.chars().count() > MAX_PLAN_TITLE_CHARS {
        return Err(format!(
            "The plan's title is too long: keep it to {MAX_PLAN_TITLE_CHARS} characters."
        ));
    }
    if steps.is_empty() {
        return Err("The plan needs at least one step.".into());
    }
    if steps.len() > MAX_PLAN_STEPS {
        return Err(format!(
            "The plan has {} steps: keep it to {MAX_PLAN_STEPS} at most (2–6 is best).",
            steps.len()
        ));
    }
    let mut trimmed = Vec::with_capacity(steps.len());
    for (i, step) in steps.iter().enumerate() {
        let step = step.trim();
        if step.is_empty() {
            return Err(format!("Step {} of the plan is empty.", i + 1));
        }
        if step.chars().count() > MAX_PLAN_STEP_CHARS {
            return Err(format!(
                "Step {} of the plan is too long: keep each step to {MAX_PLAN_STEP_CHARS} \
characters.",
                i + 1
            ));
        }
        trimmed.push(step.to_string());
    }
    Ok(ValidPlan {
        title: title.to_string(),
        steps: trimmed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORIGINS: [MessageOrigin; 4] = [
        MessageOrigin::User,
        MessageOrigin::PlanApproval,
        MessageOrigin::AutoFix,
        MessageOrigin::FirstBuild,
    ];
    const POLICIES: [PlanPolicy; 2] = [PlanPolicy::AlwaysPlan, PlanPolicy::SmallChangesDirect];

    /// Every section a prompt can hold, by the heading it starts with.
    const SECTIONS: [(&str, &str); 8] = [
        ("plan-always", "## Plans come first"),
        ("plan-small-changes", "## Plans for bigger changes"),
        (
            "origin-plan-approval",
            "## This message: the plan was approved",
        ),
        ("origin-auto-fix", "## This message: the game hit errors"),
        ("origin-first-build", "## This message: the first build"),
        ("level-balanced", "## How technical to be: balanced"),
        ("explain", "## How to finish"),
        ("teach", "## Teach me"),
    ];

    fn options(plan_policy: PlanPolicy, teach: bool, origin: MessageOrigin) -> TurnOptions {
        TurnOptions {
            role: Role::Director,
            plan_policy,
            teach,
            origin,
            model: None,
            effort: None,
            technical_level: TechnicalLevel::default(),
        }
    }

    /// The sections a combination must get (and no others).
    fn expected(o: &TurnOptions) -> Vec<&'static str> {
        let plan = match (o.origin, o.plan_policy) {
            (MessageOrigin::User, PlanPolicy::AlwaysPlan) => "plan-always",
            (MessageOrigin::User, PlanPolicy::SmallChangesDirect) => "plan-small-changes",
            (MessageOrigin::PlanApproval, _) => "origin-plan-approval",
            (MessageOrigin::AutoFix, _) => "origin-auto-fix",
            (MessageOrigin::FirstBuild, _) => "origin-first-build",
        };
        let mut v = vec![plan, "level-balanced", "explain"];
        if o.teach {
            v.push("teach");
        }
        v
    }

    #[test]
    fn every_combination_gets_exactly_its_sections_in_order() {
        for policy in POLICIES {
            for teach in [false, true] {
                for origin in ORIGINS {
                    let o = options(policy, teach, origin);
                    let prompt = system_prompt(&o, None);
                    let want = expected(&o);
                    assert!(prompt.starts_with(DIRECTOR_PROMPT.trim_end()), "{o:?}");
                    let mut last = 0;
                    for (name, heading) in SECTIONS {
                        let found = prompt.find(heading);
                        if want.contains(&name) {
                            let at = found.unwrap_or_else(|| panic!("{o:?} lacks {name}"));
                            assert!(at > last, "{o:?}: {name} out of order");
                            last = at;
                        } else {
                            assert!(found.is_none(), "{o:?} shouldn't have {name}");
                        }
                    }
                    assert!(!prompt.contains("This project's AGENTS.md"), "{o:?}");
                }
            }
        }
    }

    fn role_heading(role: Role) -> String {
        let name = match role {
            Role::Director => unreachable!("the Director has no section"),
            Role::Designer => "the Designer",
            Role::Programmer => "the Programmer",
            Role::Artist => "the Artist",
            Role::Sound => "the Sound designer",
            Role::Qa => "QA",
            Role::Producer => "the Producer",
            Role::Marketer => "the Marketer",
        };
        format!("## This message: work as {name}")
    }

    fn specialists() -> impl Iterator<Item = Role> {
        Role::ALL.into_iter().filter(|r| *r != Role::Director)
    }

    #[test]
    fn slugs_match_the_role_files() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/agent/prompts/roles");
        for role in Role::ALL {
            let file = dir.join(format!("{}.md", role.slug()));
            // The Director has no file; every other role must.
            assert_eq!(file.exists(), role != Role::Director, "{role:?}");
        }
        let files = std::fs::read_dir(&dir).unwrap().count();
        assert_eq!(files, Role::ALL.len() - 1);
    }

    #[test]
    fn every_role_file_starts_with_its_heading_and_is_finished() {
        for role in specialists() {
            let text = role_section(role).unwrap();
            assert!(
                text.starts_with(&format!("{}\n", role_heading(role))),
                "{role:?}"
            );
            let lines = text.lines().count();
            assert!((8..=24).contains(&lines), "{role:?} has {lines} lines");
            for leftover in ["TODO", "TBD", "FIXME", "XXX", "lorem", "<placeholder", "{{"] {
                assert!(!text.contains(leftover), "{role:?} contains {leftover}");
            }
            assert!(text.ends_with('\n'), "{role:?}");
            // Each role keeps to InfinaBox's rules.
            has(text, &["Don't touch git", "`addons/infinabox/`"]);
        }
    }

    #[test]
    fn every_role_section_appears_once_between_the_plan_and_explain() {
        for role in specialists() {
            for policy in POLICIES {
                for teach in [false, true] {
                    for origin in ORIGINS {
                        let o = TurnOptions {
                            role,
                            ..options(policy, teach, origin)
                        };
                        let prompt = system_prompt(&o, Some("Notes."));
                        let heading = role_heading(role);
                        assert_eq!(prompt.matches(&heading).count(), 1, "{role:?} {o:?}");
                        assert_eq!(
                            prompt.matches("## This message: work as").count(),
                            1,
                            "{role:?} {o:?}"
                        );
                        let at = prompt.find(&heading).unwrap();
                        let plan_heading = SECTIONS
                            .iter()
                            .find(|(name, _)| *name == expected(&o)[0])
                            .unwrap()
                            .1;
                        assert!(prompt.find(plan_heading).unwrap() < at, "{role:?} {o:?}");
                        assert!(
                            at < prompt.find("## How to finish").unwrap(),
                            "{role:?} {o:?}"
                        );
                        if teach {
                            assert!(at < prompt.find("## Teach me").unwrap());
                        }
                        assert!(at < prompt.find("# This project's AGENTS.md").unwrap());
                        assert!(prompt.starts_with(DIRECTOR_PROMPT.trim_end()));
                        // The rest is what the Director gets, unchanged.
                        let director =
                            system_prompt(&options(policy, teach, origin), Some("Notes."));
                        let without_role = prompt.replacen(
                            &format!("{}\n\n", role_section(role).unwrap().trim_end()),
                            "",
                            1,
                        );
                        assert_eq!(without_role, director, "{role:?} {o:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn the_director_gets_no_role_section_and_an_unchanged_prompt() {
        for policy in POLICIES {
            for teach in [false, true] {
                for origin in ORIGINS {
                    let o = options(policy, teach, origin);
                    let plan = match expected(&o)[0] {
                        "plan-always" => PLAN_ALWAYS,
                        "plan-small-changes" => PLAN_SMALL_CHANGES,
                        "origin-plan-approval" => ORIGIN_PLAN_APPROVAL,
                        "origin-auto-fix" => ORIGIN_AUTO_FIX,
                        _ => ORIGIN_FIRST_BUILD,
                    };
                    let mut want = format!(
                        "{}\n\n{}\n\n{}\n\n{}",
                        DIRECTOR_PROMPT.trim_end(),
                        plan.trim_end(),
                        LEVEL_BALANCED.trim_end(),
                        EXPLAIN.trim_end()
                    );
                    if teach {
                        want.push_str(&format!("\n\n{}", TEACH.trim_end()));
                    }
                    want.push('\n');
                    assert_eq!(system_prompt(&o, None), want, "{o:?}");
                    assert!(!want.contains("## This message: work as"));
                }
            }
        }
    }

    #[test]
    fn each_technical_level_gets_its_own_section_and_only_that_one() {
        for (level, heading) in [
            (TechnicalLevel::Guided, "## How technical to be: guided"),
            (TechnicalLevel::Balanced, "## How technical to be: balanced"),
            (TechnicalLevel::Technical, "## How technical to be: technical"),
        ] {
            let o = TurnOptions {
                technical_level: level,
                ..TurnOptions::default()
            };
            let text = turn_instructions(&o);
            assert_eq!(text.matches("## How technical to be:").count(), 1, "{level:?}");
            assert!(text.contains(heading), "{level:?}");
            // The level comes after the plan section and before "how to finish".
            assert!(text.find(heading) < text.find("## How to finish"), "{level:?}");
            assert!(text.find("## Plans come first") < text.find(heading), "{level:?}");
        }
    }

    #[test]
    fn a_resumed_turn_carries_the_role_with_its_message() {
        let o = TurnOptions {
            role: Role::Qa,
            ..TurnOptions::default()
        };
        let text = message_with_turn_instructions(&o, "Check the jump.");
        assert!(text.contains("## This message: work as QA"));
        assert!(text.ends_with("Check the jump."));
    }

    #[test]
    fn only_turns_typed_by_the_person_are_told_to_propose_plans() {
        for policy in POLICIES {
            for teach in [false, true] {
                for origin in ORIGINS {
                    let prompt = system_prompt(&options(policy, teach, origin), None);
                    assert_eq!(
                        prompt.contains(&format!("`{PROPOSE_PLAN_TOOL}`")),
                        origin == MessageOrigin::User,
                        "{policy:?} {teach} {origin:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_default_options_always_plan_without_teaching() {
        let prompt = system_prompt(&TurnOptions::default(), None);
        assert!(prompt.contains("## Plans come first"));
        assert!(!prompt.contains("## Teach me"));
    }

    /// `text` with every run of whitespace made one space, so the checks
    /// below don't depend on where the Markdown lines wrap.
    fn flat(text: &str) -> String {
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    fn has(section: &str, needles: &[&str]) {
        for needle in needles {
            assert!(flat(section).contains(needle), "{needle}\n{section}");
        }
    }

    #[test]
    fn sections_say_what_the_plan_asks_for() {
        has(
            PLAN_ALWAYS,
            &[
                "read the relevant Context cards",
                "2–6 steps",
                "no code",
                "one short sentence",
                "make no changes",
                "just answer",
            ],
        );
        has(
            PLAN_SMALL_CHANGES,
            &[
                "a color, a speed",
                "text on screen",
                "Make it directly",
                "2–6",
                "one short sentence",
                "make no changes",
                "propose a plan",
            ],
        );
        has(
            ORIGIN_PLAN_APPROVAL,
            &["most recent one", "Don't propose it again"],
        );
        has(
            ORIGIN_AUTO_FIX,
            &[
                "errors from the running game",
                "without a plan",
                "`run_game`",
                "`get_game_errors`",
            ],
        );
        has(ORIGIN_FIRST_BUILD, &["without a plan"]);
        has(
            EXPLAIN,
            &[
                "one short paragraph",
                "what you did and why",
                "what they should try",
            ],
        );
        has(
            TEACH,
            &[
                "How it works:",
                "a few sentences",
                "real scene and script files",
            ],
        );
    }

    #[test]
    fn director_prompt_keeps_the_phase_a_rules() {
        for needle in [
            "run_game",
            "get_game_errors",
            ".ibproject/context/",
            "git commits",
            "write_context_card",
        ] {
            assert!(DIRECTOR_PROMPT.contains(needle), "{needle}");
        }
    }

    #[test]
    fn agents_md_goes_last_trimmed_and_only_when_it_has_text() {
        let o = TurnOptions::default();
        let bare = system_prompt(&o, None);
        assert_eq!(system_prompt(&o, Some("  \n\n")), bare);
        let prompt = system_prompt(&o, Some("\n# My game\nUse tabs.\n\n"));
        assert!(prompt.starts_with(&bare));
        assert!(prompt.ends_with("\n---\n\n# This project's AGENTS.md\n\n# My game\nUse tabs.\n"));

        let taught = system_prompt(
            &options(PlanPolicy::AlwaysPlan, true, MessageOrigin::User),
            Some("X"),
        );
        assert!(
            taught.find("## Teach me").unwrap()
                < taught.find("# This project's AGENTS.md").unwrap()
        );
    }

    #[test]
    fn reads_agents_md_capped() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_agents_md(dir.path()), None);
        std::fs::write(dir.path().join("AGENTS.md"), "# Game\n").unwrap();
        assert_eq!(read_agents_md(dir.path()).as_deref(), Some("# Game\n"));
        let big = "a".repeat(MAX_PROJECT_INSTRUCTIONS_BYTES + 10);
        std::fs::write(dir.path().join("AGENTS.md"), &big).unwrap();
        assert_eq!(
            read_agents_md(dir.path()).unwrap().len(),
            MAX_PROJECT_INSTRUCTIONS_BYTES
        );
    }

    fn steps(s: &[&str]) -> Vec<String> {
        s.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn valid_plans_are_trimmed() {
        assert_eq!(
            validate_plan(
                "  Add a double jump ",
                &steps(&[" Let the player jump again in the air.", "Tune it.\n"])
            ),
            Ok(ValidPlan {
                title: "Add a double jump".into(),
                steps: steps(&["Let the player jump again in the air.", "Tune it."]),
            })
        );
        // The limits themselves are allowed (counted in characters, not bytes).
        let title = "é".repeat(MAX_PLAN_TITLE_CHARS);
        let step = "ü".repeat(MAX_PLAN_STEP_CHARS);
        let many = vec![step.clone(); MAX_PLAN_STEPS];
        assert!(validate_plan(&title, &many).is_ok());
        assert!(validate_plan("One step", &[step]).is_ok());
    }

    #[test]
    fn invalid_plans_say_what_to_fix() {
        let ok = steps(&["Do it."]);
        let cases: Vec<(String, Vec<String>, &str)> = vec![
            ("  ".into(), ok.clone(), "title"),
            (
                "x".repeat(MAX_PLAN_TITLE_CHARS + 1),
                ok.clone(),
                "80 characters",
            ),
            ("Plan".into(), vec![], "at least one step"),
            (
                "Plan".into(),
                vec!["a".into(); MAX_PLAN_STEPS + 1],
                "8 at most",
            ),
            ("Plan".into(), steps(&["Fine.", "   "]), "Step 2"),
            (
                "Plan".into(),
                vec!["a".repeat(MAX_PLAN_STEP_CHARS + 1)],
                "200",
            ),
        ];
        for (title, s, needle) in cases {
            let err = validate_plan(&title, &s).unwrap_err();
            assert!(err.contains(needle), "{err}");
        }
    }
}
