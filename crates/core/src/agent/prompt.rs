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
//! then always `explain.md` ("what I did and why"), then `teach.md` when
//! "Teach me" is on, then the project's `AGENTS.md`.
//!
//! This module also holds the limits of a `propose_plan` call, so the MCP
//! server that accepts one and the stream parsers that turn it into
//! `AgentEvent::PlanProposed` agree on what a valid plan is.

use std::path::Path;

use super::types::{MessageOrigin, PlanPolicy, TurnOptions};

/// The Director system prompt: who the agent is talking to and how it works.
pub const DIRECTOR_PROMPT: &str = include_str!("prompts/director.md");

const PLAN_ALWAYS: &str = include_str!("prompts/plan-always.md");
const PLAN_SMALL_CHANGES: &str = include_str!("prompts/plan-small-changes.md");
const ORIGIN_PLAN_APPROVAL: &str = include_str!("prompts/origin-plan-approval.md");
const ORIGIN_AUTO_FIX: &str = include_str!("prompts/origin-auto-fix.md");
const ORIGIN_FIRST_BUILD: &str = include_str!("prompts/origin-first-build.md");
const EXPLAIN: &str = include_str!("prompts/explain.md");
const TEACH: &str = include_str!("prompts/teach.md");

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
    let plan = match (options.origin, options.plan_policy) {
        (MessageOrigin::User, PlanPolicy::AlwaysPlan) => PLAN_ALWAYS,
        (MessageOrigin::User, PlanPolicy::SmallChangesDirect) => PLAN_SMALL_CHANGES,
        (MessageOrigin::PlanApproval, _) => ORIGIN_PLAN_APPROVAL,
        (MessageOrigin::AutoFix, _) => ORIGIN_AUTO_FIX,
        (MessageOrigin::FirstBuild, _) => ORIGIN_FIRST_BUILD,
    };
    let mut sections = vec![
        DIRECTOR_PROMPT.trim_end(),
        plan.trim_end(),
        EXPLAIN.trim_end(),
    ];
    if options.teach {
        sections.push(TEACH.trim_end());
    }
    let mut prompt = sections.join("\n\n");
    prompt.push('\n');
    if let Some(text) = agents_md.map(str::trim).filter(|t| !t.is_empty()) {
        prompt.push_str(&format!(
            "\n---\n\n# This project's {PROJECT_INSTRUCTIONS_FILE}\n\n{text}\n"
        ));
    }
    prompt
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
    const SECTIONS: [(&str, &str); 7] = [
        ("plan-always", "## Plans come first"),
        ("plan-small-changes", "## Plans for bigger changes"),
        (
            "origin-plan-approval",
            "## This message: the plan was approved",
        ),
        ("origin-auto-fix", "## This message: the game hit errors"),
        ("origin-first-build", "## This message: the first build"),
        ("explain", "## How to finish"),
        ("teach", "## Teach me"),
    ];

    fn options(plan_policy: PlanPolicy, teach: bool, origin: MessageOrigin) -> TurnOptions {
        TurnOptions {
            plan_policy,
            teach,
            origin,
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
        let mut v = vec![plan, "explain"];
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
