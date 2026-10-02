//! The app's side of the playtest channel: the running game (through the
//! InfinaBox addon's runtime script, `godot-addon/infinabox/
//! infinabox_runtime.gd`) listens on a loopback port for a list of steps —
//! press an input, wait, take a picture, read a value, check it — and
//! answers with a report. The AI asks for that through the `playtest` MCP
//! tool; the app relays it here.
//!
//! The game learns its port, a per-launch token and the folder for pictures
//! from environment variables (never the command line, which other users on
//! the machine can read). Pictures go to `.ibproject/playtest/`, which is
//! git-ignored and emptied on every run.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde_json::{Value, json};

pub const ENV_PORT: &str = "INFINABOX_PLAYTEST_PORT";
pub const ENV_TOKEN: &str = "INFINABOX_PLAYTEST_TOKEN";
pub const ENV_DIR: &str = "INFINABOX_PLAYTEST_DIR";

/// Where pictures are saved, relative to the project.
pub const PLAYTEST_DIR: &str = ".ibproject/playtest";

pub const MAX_STEPS: usize = 60;
/// The most waiting (holding inputs included) one test may ask for, in seconds.
pub const MAX_TOTAL_SECONDS: f64 = 120.0;
const MAX_REPORT_BYTES: u64 = 4 * 1024 * 1024;
/// How long a game that is still starting gets to open its port.
const CONNECT_PATIENCE: Duration = Duration::from_secs(12);
/// Slack on top of the test's own waiting for the game to answer.
const ANSWER_SLACK: Duration = Duration::from_secs(25);

/// Where one run of the game listens, and how to prove who is asking.
#[derive(Clone, Debug)]
pub struct Endpoint {
    pub port: u16,
    pub token: String,
    pub dir: PathBuf,
}

impl Endpoint {
    /// Picks a free port and a token for a run of `project`, and empties the
    /// pictures folder of the last run.
    pub fn new(project: &Path) -> Result<Self> {
        let port = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .context("couldn't find a free port")?
            .local_addr()?
            .port();
        let token = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
        let dir = project.join(PLAYTEST_DIR);
        if dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    if entry.path().extension().is_some_and(|e| e == "png") {
                        let _ = std::fs::remove_file(entry.path());
                    }
                }
            }
        }
        std::fs::create_dir_all(&dir).context("couldn't make the playtest folder")?;
        Ok(Self { port, token, dir })
    }

    /// The environment the game is started with.
    pub fn env(&self) -> Vec<(String, String)> {
        vec![
            (ENV_PORT.into(), self.port.to_string()),
            (ENV_TOKEN.into(), self.token.clone()),
            (ENV_DIR.into(), self.dir.to_string_lossy().into_owned()),
        ]
    }
}

/// How long the steps will take, by what they ask for.
fn estimated_seconds(steps: &[Value]) -> f64 {
    steps
        .iter()
        .map(|s| {
            let action = s.get("action").and_then(Value::as_str).unwrap_or("");
            let seconds = s.get("seconds").and_then(Value::as_f64).unwrap_or(match action {
                "wait" => 0.5,
                "press" | "key" => 0.1,
                _ => 0.0,
            });
            let fixed = match action {
                "screenshot" => 0.3,
                "click" => 0.1,
                _ => 0.05,
            };
            match action {
                "press" | "key" | "wait" => seconds.clamp(0.0, 30.0) + fixed,
                _ => fixed,
            }
        })
        .sum()
}

/// Checks a test before the game is bothered with it.
pub fn validate_steps(steps: &[Value]) -> Result<(), String> {
    if steps.is_empty() || steps.len() > MAX_STEPS {
        return Err(format!("Send between 1 and {MAX_STEPS} steps."));
    }
    for (i, step) in steps.iter().enumerate() {
        let action = step.get("action").and_then(Value::as_str);
        if !matches!(
            action,
            Some("press" | "key" | "click" | "wait" | "screenshot" | "tree" | "get" | "expect" | "info")
        ) {
            return Err(format!(
                "Step {} needs an action: press, key, click, wait, screenshot, tree, get, expect or info.",
                i + 1
            ));
        }
    }
    let total = estimated_seconds(steps);
    if total > MAX_TOTAL_SECONDS {
        return Err(format!(
            "That test would take about {total:.0} seconds; keep it under {MAX_TOTAL_SECONDS:.0}. \
             Split it into several tests."
        ));
    }
    Ok(())
}

fn connect(port: u16) -> Result<TcpStream, String> {
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let deadline = Instant::now() + CONNECT_PATIENCE;
    loop {
        match TcpStream::connect_timeout(&addr, Duration::from_secs(2)) {
            Ok(stream) => return Ok(stream),
            Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(250)),
            Err(_) => {
                return Err("The game isn't taking playtest steps. It may still be starting, \
                    or it was started before playtesting was available: run it again with run_game."
                    .into());
            }
        }
    }
}

/// Runs a test on the running game and returns its report:
/// `{ "passed": bool, "steps": [...] }`, each step with `ok` and what it
/// found, pictures as project-relative `path`s.
pub fn run_steps(endpoint: &Endpoint, project: &Path, steps: &[Value]) -> Result<Value, String> {
    validate_steps(steps)?;
    let mut stream = connect(endpoint.port)?;
    let wait = Duration::from_secs_f64(estimated_seconds(steps)) + ANSWER_SLACK;
    stream.set_read_timeout(Some(wait)).ok();
    stream.set_write_timeout(Some(Duration::from_secs(5))).ok();
    let request = json!({ "token": endpoint.token, "steps": steps });
    stream
        .write_all(format!("{request}\n").as_bytes())
        .map_err(|e| format!("The game closed the connection: {e}"))?;

    let mut line = String::new();
    let read = BufReader::new((&stream).take(MAX_REPORT_BYTES)).read_line(&mut line);
    let stopped = "The game stopped (or crashed) before the test finished. \
        Check get_game_errors and get_game_output.";
    match read {
        Ok(0) => return Err(stopped.into()),
        Ok(_) => {}
        Err(e) if matches!(e.kind(), std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock) => {
            return Err("The game didn't answer in time. It may be stuck: check get_game_errors.".into());
        }
        Err(_) => return Err(stopped.into()),
    }
    let reply: Value = serde_json::from_str(line.trim()).map_err(|_| "The game sent back something unreadable.".to_string())?;
    if reply.get("ok") != Some(&Value::Bool(true)) {
        let why = reply.get("error").and_then(Value::as_str).unwrap_or("The game refused the test.");
        return Err(why.to_string());
    }
    let mut report = reply.get("report").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut passed = true;
    for (i, entry) in report.iter_mut().enumerate() {
        passed &= entry.get("ok") == Some(&Value::Bool(true));
        let Some(obj) = entry.as_object_mut() else { continue };
        obj.insert("step".into(), json!(i + 1));
        if let Some(path) = obj.get("path").and_then(Value::as_str).map(PathBuf::from) {
            let rel = path.strip_prefix(project).unwrap_or(&path);
            obj.insert("path".into(), json!(rel.to_string_lossy().replace('\\', "/")));
        }
    }
    Ok(json!({ "passed": passed, "steps": report }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::thread;

    /// Stands in for the addon: answers one request with `reply` (given the
    /// request line) and returns the request it saw.
    fn fake_game(reply: impl FnOnce(&str) -> Option<String> + Send + 'static) -> (u16, thread::JoinHandle<String>) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut line = String::new();
            BufReader::new(&stream).read_line(&mut line).unwrap();
            if let Some(out) = reply(&line) {
                stream.write_all(format!("{out}\n").as_bytes()).unwrap();
            }
            line
        });
        (port, handle)
    }

    fn endpoint(port: u16, dir: &Path) -> Endpoint {
        Endpoint { port, token: "tok".into(), dir: dir.join(PLAYTEST_DIR) }
    }

    #[test]
    fn a_new_endpoint_has_a_port_a_token_and_a_clean_folder() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(PLAYTEST_DIR)).unwrap();
        std::fs::write(dir.path().join(PLAYTEST_DIR).join("shot-001.png"), b"x").unwrap();
        std::fs::write(dir.path().join(PLAYTEST_DIR).join("notes.txt"), b"keep").unwrap();
        let a = Endpoint::new(dir.path()).unwrap();
        let b = Endpoint::new(dir.path()).unwrap();
        assert_ne!(a.token, b.token);
        assert!(a.token.len() >= 64);
        assert!(!dir.path().join(PLAYTEST_DIR).join("shot-001.png").exists());
        assert!(dir.path().join(PLAYTEST_DIR).join("notes.txt").exists());
        let env = a.env();
        assert!(env.iter().any(|(k, v)| k == ENV_PORT && *v == a.port.to_string()));
        assert!(env.iter().any(|(k, _)| k == ENV_TOKEN));
        assert!(env.iter().any(|(k, _)| k == ENV_DIR));
    }

    #[test]
    fn steps_are_checked_before_the_game_is_asked() {
        assert!(validate_steps(&[]).is_err());
        assert!(validate_steps(&[json!({"action": "dance"})]).is_err());
        assert!(validate_steps(&[json!({"nothing": 1})]).is_err());
        assert!(validate_steps(&[json!({"action": "press", "input": "jump"})]).is_ok());
        let long: Vec<Value> = (0..5).map(|_| json!({"action": "wait", "seconds": 30})).collect();
        assert!(validate_steps(&long).unwrap_err().contains("Split it"));
        let many: Vec<Value> = (0..=MAX_STEPS).map(|_| json!({"action": "info"})).collect();
        assert!(validate_steps(&many).is_err());
    }

    #[test]
    fn a_report_comes_back_with_step_numbers_pass_state_and_relative_pictures() {
        let dir = tempfile::tempdir().unwrap();
        let shot = dir.path().join(PLAYTEST_DIR).join("shot-001.png");
        let shot_str = shot.to_string_lossy().replace('\\', "\\\\");
        let reply = format!(
            "{{\"ok\":true,\"report\":[{{\"action\":\"screenshot\",\"ok\":true,\"path\":\"{shot_str}\"}},\
             {{\"action\":\"expect\",\"ok\":false,\"message\":\"nope\"}}]}}"
        );
        let (port, seen) = fake_game(move |_| Some(reply));
        let steps = [json!({"action": "screenshot"}), json!({"action": "expect", "node": "P", "property": "x", "value": 1})];
        let report = run_steps(&endpoint(port, dir.path()), dir.path(), &steps).unwrap();
        assert_eq!(report["passed"], false);
        assert_eq!(report["steps"][0]["step"], 1);
        assert_eq!(report["steps"][0]["path"], ".ibproject/playtest/shot-001.png");
        assert_eq!(report["steps"][1]["message"], "nope");
        // The game was sent the token and the steps.
        let request: Value = serde_json::from_str(&seen.join().unwrap()).unwrap();
        assert_eq!(request["token"], "tok");
        assert_eq!(request["steps"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn a_passing_test_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let (port, _) = fake_game(|_| Some(r#"{"ok":true,"report":[{"action":"info","ok":true}]}"#.into()));
        let report = run_steps(&endpoint(port, dir.path()), dir.path(), &[json!({"action": "info"})]).unwrap();
        assert_eq!(report["passed"], true);
    }

    #[test]
    fn a_game_that_stops_or_refuses_is_an_error_not_a_pass() {
        let dir = tempfile::tempdir().unwrap();
        let steps = [json!({"action": "info"})];
        // Closes the connection without answering.
        let (port, _) = fake_game(|_| None);
        let err = run_steps(&endpoint(port, dir.path()), dir.path(), &steps).unwrap_err();
        assert!(err.contains("stopped"), "{err}");
        // Refuses the token.
        let (port, _) = fake_game(|_| Some(r#"{"ok":false,"error":"Not a playtest request."}"#.into()));
        let err = run_steps(&endpoint(port, dir.path()), dir.path(), &steps).unwrap_err();
        assert_eq!(err, "Not a playtest request.");
        // Garbage.
        let (port, _) = fake_game(|_| Some("hello".into()));
        assert!(run_steps(&endpoint(port, dir.path()), dir.path(), &steps).unwrap_err().contains("unreadable"));
    }

    /// The real addon in the real Godot, headless (no screenshot possible
    /// there, which it must say). Pictures and input handling are covered by
    /// the app's end-to-end run under a display.
    #[test]
    #[ignore = "needs a real Godot (INFINABOX_GODOT); run with --ignored"]
    fn the_real_addon_answers_a_test() {
        use crate::godot::run::GameProcess;
        let godot = crate::godot::test_support::real_godot();
        let tmp = tempfile::tempdir().unwrap();
        let project = crate::scaffold::create_project(tmp.path(), "Probe").unwrap();
        // Starting it imports nothing, but the addon must be registered.
        crate::scaffold::ensure_addon(&project).unwrap();
        let endpoint = Endpoint::new(&project).unwrap();
        let mut game = GameProcess::start_with_env(
            &godot,
            &project,
            &["--headless".to_string()],
            &endpoint.env(),
            |_| {},
            |_| {},
        )
        .unwrap();
        let steps = [
            json!({"action": "info"}),
            json!({"action": "press", "input": "no_such_action"}),
            json!({"action": "wait", "seconds": 0.2}),
            json!({"action": "expect", "node": "/root/InfinaBox", "op": "exists"}),
            json!({"action": "expect", "node": "/root/Nothing", "op": "exists"}),
            json!({"action": "tree", "depth": 1}),
            json!({"action": "screenshot"}),
        ];
        let report = run_steps(&endpoint, &project, &steps).unwrap();
        game.stop().unwrap();
        let ok: Vec<bool> = report["steps"].as_array().unwrap().iter().map(|s| s["ok"] == true).collect();
        assert_eq!(ok, [true, false, true, true, false, true, false], "{report:#}");
        assert_eq!(report["passed"], false);
        assert!(report["steps"][1]["message"].as_str().unwrap().contains("no_such_action"));
        assert!(report["steps"][6]["message"].as_str().unwrap().contains("no picture"), "{report:#}");
    }
}
