//! Headless boot check: imports the project's assets, runs it for a few
//! frames without a window, and returns the errors Godot printed.
//!
//! Flags confirmed against the real 4.7.2 `--help`: `--headless` (no window,
//! dummy audio), `--quit-after <int>` (iterations), and `--import` ("starts
//! the editor, waits for any resources to be imported, and then quits").
//!
//! The import step matters: a project run with `--path` never imports new
//! assets itself, so a freshly written `icon.png` fails to load with
//! `No loader found for resource` until `--import` has run once (checked
//! against the real 4.7.2 binary). Import writes Godot's usual `.godot/`
//! cache folder into the project, exactly as the Godot editor would.

use std::path::Path;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result, bail};

use super::errors::ErrorParser;
use super::run::GameProcess;
use super::types::{GameError, GameOutputLine, GameState};

/// Frames to run for; matches how the Task 0.2 fixtures were recorded.
pub const BOOT_CHECK_FRAMES: u32 = 30;
/// Importing a big project for the first time can take a while.
const IMPORT_TIMEOUT: Duration = Duration::from_secs(300);
const RUN_TIMEOUT: Duration = Duration::from_secs(120);

pub fn boot_check(godot: &Path, project: &Path) -> Result<Vec<GameError>> {
    if !project.join("project.godot").is_file() {
        bail!(
            "{} isn't a Godot project (no project.godot)",
            project.display()
        );
    }
    let mut errors = import_assets(godot, project)?;
    let run_args = vec![
        "--headless".to_string(),
        "--quit-after".to_string(),
        BOOT_CHECK_FRAMES.to_string(),
    ];
    for error in run_headless(godot, project, &run_args, RUN_TIMEOUT)? {
        // A broken script is reported by both the import and the run; keep
        // one copy of each identical block.
        if !errors.iter().any(|e| e.raw == error.raw) {
            errors.push(error);
        }
    }
    Ok(errors)
}

/// Runs `godot --headless --path <project> --import` so every asset is
/// imported before the game runs, returning any errors it printed (script
/// parse errors show up here too).
pub fn import_assets(godot: &Path, project: &Path) -> Result<Vec<GameError>> {
    let args = vec!["--headless".to_string(), "--import".to_string()];
    run_headless(godot, project, &args, IMPORT_TIMEOUT)
}

/// Runs Godot to completion with `args`, parsing its stderr. A non-zero
/// exit is an error carrying Godot's last output lines, since then Godot
/// itself failed rather than just reporting problems in the game.
fn run_headless(
    godot: &Path,
    project: &Path,
    args: &[String],
    timeout: Duration,
) -> Result<Vec<GameError>> {
    let lines: Arc<Mutex<Vec<GameOutputLine>>> = Arc::default();
    let sink = lines.clone();
    let (exit_tx, exit_rx) = mpsc::channel();
    let mut game = GameProcess::start_with_args(
        godot,
        project,
        args,
        move |line| sink.lock().unwrap().push(line),
        move |state| {
            let _ = exit_tx.send(state);
        },
    )?;

    let state = match exit_rx.recv_timeout(timeout) {
        Ok(state) => state,
        Err(_) => {
            game.stop()?;
            bail!(
                "Godot didn't finish `{}` within {}s",
                args.join(" "),
                timeout.as_secs()
            );
        }
    };
    let lines = std::mem::take(&mut *lines.lock().unwrap());

    if state == GameState::Crashed {
        let status = game
            .exit_status()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "an unknown status".into());
        let tail: Vec<&str> = lines
            .iter()
            .rev()
            .take(20)
            .rev()
            .map(|l| l.text.as_str())
            .collect();
        return Err(anyhow::anyhow!("{}", tail.join("\n")))
            .context(format!("Godot `{}` exited with {status}", args.join(" ")));
    }

    let mut parser = ErrorParser::new();
    let mut errors: Vec<GameError> = lines.iter().flat_map(|l| parser.push(l)).collect();
    errors.extend(parser.finish());
    Ok(errors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::godot::test_support::{fixture_project, real_godot};

    #[test]
    fn a_folder_without_project_godot_is_rejected_before_running_anything() {
        let dir = tempfile::tempdir().unwrap();
        let err = boot_check(Path::new("/no/such/godot"), dir.path()).unwrap_err();
        assert!(err.to_string().contains("isn't a Godot project"), "{err}");
    }

    #[test]
    #[ignore = "needs a real Godot (INFINABOX_GODOT); run with --ignored"]
    fn boot_check_on_real_fixture_projects() {
        let godot = real_godot();

        let clean = fixture_project("clean");
        assert_eq!(boot_check(&godot, clean.path()).unwrap(), vec![]);

        let parse = fixture_project("parse_error");
        let errors = boot_check(&godot, parse.path()).unwrap();
        let located: Vec<_> = errors.iter().map(|e| (e.file.as_deref(), e.line)).collect();
        assert_eq!(
            located,
            vec![(Some("res://main.gd"), Some(4)), (None, None)]
        );
        assert!(
            errors[0]
                .message
                .starts_with("Parse Error: Expected expression")
        );

        let runtime = fixture_project("runtime_error");
        let errors = boot_check(&godot, runtime.path()).unwrap();
        assert_eq!(errors.len(), 1);
        assert_eq!(
            (
                errors[0].message.as_str(),
                errors[0].file.as_deref(),
                errors[0].line
            ),
            (
                "Invalid call. Nonexistent function 'jump' in base 'Nil'.",
                Some("res://main.gd"),
                Some(7)
            )
        );

        let missing = fixture_project("missing_resource");
        let errors = boot_check(&godot, missing.path()).unwrap();
        assert_eq!(errors.len(), 1);
        assert_eq!(
            (errors[0].file.as_deref(), errors[0].line),
            (Some("res://main.gd"), Some(4))
        );
    }

    #[test]
    #[ignore = "needs a real Godot (INFINABOX_GODOT); run with --ignored"]
    fn warnings_and_push_error_from_a_real_godot_parse_as_expected() {
        let godot = real_godot();
        let project = tempfile::tempdir().unwrap();
        std::fs::write(
            project.path().join("project.godot"),
            "config_version=5\n\n[application]\n\nrun/main_scene=\"res://main.tscn\"\n",
        )
        .unwrap();
        std::fs::write(
            project.path().join("main.tscn"),
            "[gd_scene load_steps=2 format=3]\n\n[ext_resource type=\"Script\" path=\"res://main.gd\" id=\"1\"]\n\n[node name=\"Main\" type=\"Node2D\"]\nscript = ExtResource(\"1\")\n",
        )
        .unwrap();
        std::fs::write(
            project.path().join("main.gd"),
            "extends Node2D\n\nfunc _ready() -> void:\n\tpush_warning(\"just a warning\")\n\tpush_error(\"player missing\")\n",
        )
        .unwrap();

        let errors = boot_check(&godot, project.path()).unwrap();
        // The warning is swallowed; push_error is an error at the call site.
        assert_eq!(errors.len(), 1, "{errors:#?}");
        assert_eq!(errors[0].message, "player missing");
        assert_eq!(
            (errors[0].file.as_deref(), errors[0].line),
            (Some("res://main.gd"), Some(5))
        );
    }
}
