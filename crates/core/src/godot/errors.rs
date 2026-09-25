//! Incremental parser turning real Godot output lines into `GameError`s,
//! plus bounded buffers of recent errors/output for the bridge.
//!
//! Matched against the real Godot 4.7.2 recordings in
//! `tests/fixtures/godot/` (see that folder's README). What those show:
//!
//! - Errors go to **stderr** as multi-line blocks: a header line
//!   (`SCRIPT ERROR: ...`, `ERROR: ...`), then an indented
//!   `at: <function> (<location>)` line, sometimes followed by
//!   `GDScript backtrace (most recent call first):` and indented
//!   `[n] <function> (res://file.gd:line)` frames.
//! - For `SCRIPT ERROR`s the `at:` location is the user's `res://` file. For
//!   engine `ERROR`s it's engine C++ source, and the user's location is the
//!   first backtrace frame instead.
//! - Exit code says nothing: Godot exits 0 even when scripts fail.
//!
//! A block is only known to be complete when the next non-continuation
//! stderr line arrives, so the last error of a burst waits in the parser.
//! Callers streaming a live game should call [`ErrorParser::finish`] once
//! stderr has gone quiet for a moment (and always at exit) — see
//! [`ErrorParser::has_pending`].

use std::collections::VecDeque;
use std::sync::OnceLock;

use regex::Regex;

use super::types::{GameError, GameOutputLine, OutputStream};

/// `SCRIPT ERROR: msg`, `ERROR: msg`, `USER ERROR: msg` (from `push_error`),
/// `USER SCRIPT ERROR: msg`, `SHADER ERROR: msg`.
fn error_header() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^(?:USER )?(?:SCRIPT |SHADER )?ERROR: (.*)$").unwrap())
}

/// Warnings share the error block shape (header + `at:` + backtrace). They
/// aren't `GameError`s, but their continuation lines must still be consumed
/// so they never get attached to a neighbouring error.
fn warning_header() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^(?:USER )?(?:SCRIPT |SHADER )?WARNING: (.*)$").unwrap())
}

/// `at: <function> (<path>:<line>)`, trimmed. The path is greedy so
/// `res://a:b.gd`-style colons stay in it; the line is the last `:<digits>`.
fn at_line() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^at: .*\((.+):(\d+)\)$").unwrap())
}

/// `[n] <function> (<path>:<line>)`, trimmed.
fn backtrace_frame() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^\[\d+\] .*\((.+):(\d+)\)$").unwrap())
}

fn is_continuation(text: &str) -> bool {
    if !text.starts_with([' ', '\t']) {
        return false;
    }
    let t = text.trim();
    t.starts_with("at:") || t.starts_with("GDScript backtrace") || backtrace_frame().is_match(t)
}

/// A user location, if `path` is inside the project (`res://`). Engine
/// source paths (`core/io/resource_loader.cpp`) aren't useful to the user.
fn user_location(re: &Regex, text: &str) -> Option<(String, u32)> {
    let caps = re.captures(text.trim())?;
    let path = caps.get(1)?.as_str();
    if !path.starts_with("res://") {
        return None;
    }
    let line = caps.get(2)?.as_str().parse().ok()?;
    Some((path.to_string(), line))
}

struct PendingBlock {
    /// `None` for a warning block we're only swallowing.
    message: Option<String>,
    raw: Vec<String>,
    at_location: Option<(String, u32)>,
    frame_location: Option<(String, u32)>,
}

impl PendingBlock {
    fn into_error(self) -> Option<GameError> {
        let message = self.message?;
        // The `at:` line wins when it's the user's file (script errors);
        // otherwise the innermost backtrace frame (engine errors).
        let location = self.at_location.or(self.frame_location);
        Some(GameError {
            message,
            file: location.as_ref().map(|(f, _)| f.clone()),
            line: location.map(|(_, l)| l),
            raw: self.raw.join("\n"),
        })
    }
}

#[derive(Default)]
pub struct ErrorParser {
    pending: Option<PendingBlock>,
}

impl ErrorParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds one output line; returns any errors completed by it.
    ///
    /// Only stderr is parsed: stdout and stderr arrive from separate reader
    /// threads, so a stdout line says nothing about whether a stderr block
    /// has ended.
    pub fn push(&mut self, line: &GameOutputLine) -> Vec<GameError> {
        if line.stream != OutputStream::Stderr {
            return Vec::new();
        }
        let text = line.text.trim_end_matches(['\r', '\n']);

        if let Some(block) = self.pending.as_mut()
            && is_continuation(text)
        {
            block.raw.push(text.to_string());
            let trimmed = text.trim();
            if trimmed.starts_with("at:") {
                if block.at_location.is_none() {
                    block.at_location = user_location(at_line(), trimmed);
                }
            } else if block.frame_location.is_none() {
                // Frames are most-recent-call first, so the first one with
                // a `res://` location is where the user's code was.
                block.frame_location = user_location(backtrace_frame(), trimmed);
            }
            return Vec::new();
        }

        // Anything else ends the pending block (if any).
        let out = self.finish();
        if let Some(caps) = error_header().captures(text) {
            self.pending = Some(PendingBlock {
                message: Some(caps[1].trim().to_string()),
                raw: vec![text.to_string()],
                at_location: None,
                frame_location: None,
            });
        } else if warning_header().is_match(text) {
            self.pending = Some(PendingBlock {
                message: None,
                raw: vec![text.to_string()],
                at_location: None,
                frame_location: None,
            });
        }
        out
    }

    /// Flushes an error still waiting for more continuation lines (e.g. at
    /// exit, or once stderr has gone quiet).
    pub fn finish(&mut self) -> Vec<GameError> {
        self.pending
            .take()
            .and_then(PendingBlock::into_error)
            .into_iter()
            .collect()
    }

    /// True when an error block has started but not been emitted yet; the
    /// caller should `finish()` it if no more stderr arrives shortly.
    pub fn has_pending(&self) -> bool {
        self.pending.as_ref().is_some_and(|b| b.message.is_some())
    }
}

/// Bounded history of recent output and errors, oldest dropped first.
pub struct RecentLog {
    pub max_lines: usize,
    pub max_errors: usize,
    lines: VecDeque<GameOutputLine>,
    errors: VecDeque<GameError>,
}

impl RecentLog {
    pub fn new(max_lines: usize, max_errors: usize) -> Self {
        Self {
            max_lines,
            max_errors,
            lines: VecDeque::new(),
            errors: VecDeque::new(),
        }
    }

    pub fn push_line(&mut self, line: GameOutputLine) {
        push_bounded(&mut self.lines, line, self.max_lines);
    }

    pub fn push_error(&mut self, error: GameError) {
        push_bounded(&mut self.errors, error, self.max_errors);
    }

    /// The last `n` lines, oldest first.
    pub fn recent_lines(&self, n: usize) -> Vec<GameOutputLine> {
        last_n(&self.lines, n)
    }

    /// The last `n` errors, oldest first.
    pub fn recent_errors(&self, n: usize) -> Vec<GameError> {
        last_n(&self.errors, n)
    }

    /// Forgets everything (e.g. when a new run starts).
    pub fn clear(&mut self) {
        self.lines.clear();
        self.errors.clear();
    }
}

fn push_bounded<T>(buf: &mut VecDeque<T>, item: T, max: usize) {
    if max == 0 {
        return;
    }
    while buf.len() >= max {
        buf.pop_front();
    }
    buf.push_back(item);
}

fn last_n<T: Clone>(buf: &VecDeque<T>, n: usize) -> Vec<T> {
    buf.iter()
        .skip(buf.len().saturating_sub(n))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> String {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/godot")
            .join(name);
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    }

    fn lines(stream: OutputStream, text: &str) -> Vec<GameOutputLine> {
        text.lines()
            .map(|l| GameOutputLine {
                stream,
                text: l.to_string(),
            })
            .collect()
    }

    /// Feeds a scenario's real stdout and stderr the way reader threads
    /// might: stdout first, then stderr, then `finish()`.
    fn parse_scenario(scenario: &str) -> Vec<GameError> {
        let mut parser = ErrorParser::new();
        let mut errors = Vec::new();
        let out = fixture(&format!("{scenario}.stdout.txt"));
        let err = fixture(&format!("{scenario}.stderr.txt"));
        for line in lines(OutputStream::Stdout, &out)
            .into_iter()
            .chain(lines(OutputStream::Stderr, &err))
        {
            errors.extend(parser.push(&line));
        }
        errors.extend(parser.finish());
        errors
    }

    #[test]
    fn clean_run_has_no_errors() {
        assert_eq!(parse_scenario("clean"), vec![]);
    }

    #[test]
    fn parse_error_yields_the_script_error_and_the_load_failure() {
        let errors = parse_scenario("parse_error");
        assert_eq!(
            errors,
            vec![
                GameError {
                    message: r#"Parse Error: Expected expression after "+" operator."#.into(),
                    file: Some("res://main.gd".into()),
                    line: Some(4),
                    raw: "SCRIPT ERROR: Parse Error: Expected expression after \"+\" operator.\n          at: GDScript::reload (res://main.gd:4)".into(),
                },
                GameError {
                    message: r#"Failed to load script "res://main.gd" with error "Parse error"."#.into(),
                    // Godot printed only an engine location for this one.
                    file: None,
                    line: None,
                    raw: "ERROR: Failed to load script \"res://main.gd\" with error \"Parse error\".\n   at: load (modules/gdscript/gdscript_resource_format.cpp:46)".into(),
                },
            ]
        );
    }

    #[test]
    fn runtime_error_uses_the_at_location_and_keeps_the_backtrace_raw() {
        let errors = parse_scenario("runtime_error");
        assert_eq!(
            errors,
            vec![GameError {
                message: "Invalid call. Nonexistent function 'jump' in base 'Nil'.".into(),
                file: Some("res://main.gd".into()),
                line: Some(7),
                raw: fixture("runtime_error.stderr.txt").trim_end().into(),
            }]
        );
    }

    #[test]
    fn engine_error_takes_the_user_location_from_the_first_backtrace_frame() {
        let errors = parse_scenario("missing_resource");
        assert_eq!(
            errors,
            vec![GameError {
                message:
                    "Resource file not found: res://art/does_not_exist.png (expected type: unknown)"
                        .into(),
                file: Some("res://main.gd".into()),
                line: Some(4),
                raw: fixture("missing_resource.stderr.txt").trim_end().into(),
            }]
        );
    }

    #[test]
    fn a_block_is_emitted_when_the_next_header_arrives_not_before() {
        let mut parser = ErrorParser::new();
        let err = lines(OutputStream::Stderr, &fixture("parse_error.stderr.txt"));
        assert_eq!(parser.push(&err[0]), vec![]);
        assert!(parser.has_pending());
        assert_eq!(parser.push(&err[1]), vec![]);
        // The second header completes the first block.
        let first = parser.push(&err[2]);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].line, Some(4));
        assert_eq!(parser.push(&err[3]), vec![]);
        assert_eq!(parser.finish().len(), 1);
        assert!(!parser.has_pending());
        assert_eq!(parser.finish(), vec![]);
    }

    #[test]
    fn interleaved_stdout_does_not_break_a_stderr_block() {
        let mut parser = ErrorParser::new();
        let err = lines(OutputStream::Stderr, &fixture("runtime_error.stderr.txt"));
        let stdout = GameOutputLine {
            stream: OutputStream::Stdout,
            text: "starting".into(),
        };
        let mut errors = Vec::new();
        for (i, line) in err.iter().enumerate() {
            errors.extend(parser.push(line));
            if i == 1 {
                errors.extend(parser.push(&stdout));
            }
        }
        errors.extend(parser.finish());
        assert_eq!(errors.len(), 1);
        assert_eq!(
            errors[0].raw,
            fixture("runtime_error.stderr.txt").trim_end()
        );
    }

    #[test]
    fn ordinary_stderr_lines_end_a_block_without_becoming_errors() {
        let mut parser = ErrorParser::new();
        let mut errors = Vec::new();
        for line in lines(OutputStream::Stderr, &fixture("runtime_error.stderr.txt")) {
            errors.extend(parser.push(&line));
        }
        errors.extend(parser.push(&GameOutputLine {
            stream: OutputStream::Stderr,
            text: "some unrelated stderr text".into(),
        }));
        assert_eq!(errors.len(), 1);
        assert!(!parser.has_pending());
        assert_eq!(parser.finish(), vec![]);
    }

    fn line(i: usize) -> GameOutputLine {
        GameOutputLine {
            stream: OutputStream::Stdout,
            text: format!("line {i}"),
        }
    }

    fn error(i: usize) -> GameError {
        GameError {
            message: format!("e{i}"),
            file: None,
            line: None,
            raw: format!("ERROR: e{i}"),
        }
    }

    #[test]
    fn recent_log_keeps_only_the_newest_entries_oldest_first() {
        let mut log = RecentLog::new(3, 2);
        for i in 0..5 {
            log.push_line(line(i));
            log.push_error(error(i));
        }
        assert_eq!(log.recent_lines(10), vec![line(2), line(3), line(4)]);
        assert_eq!(log.recent_lines(2), vec![line(3), line(4)]);
        assert_eq!(log.recent_lines(0), vec![]);
        assert_eq!(log.recent_errors(10), vec![error(3), error(4)]);
        assert_eq!(log.recent_errors(1), vec![error(4)]);

        log.clear();
        assert_eq!(log.recent_lines(10), vec![]);
        assert_eq!(log.recent_errors(10), vec![]);
    }

    #[test]
    fn recent_log_with_zero_capacity_stores_nothing() {
        let mut log = RecentLog::new(0, 0);
        log.push_line(line(1));
        log.push_error(error(1));
        assert_eq!(log.recent_lines(5), vec![]);
        assert_eq!(log.recent_errors(5), vec![]);
    }
}
