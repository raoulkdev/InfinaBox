//! Chat threads persisted as `.ibproject/chat/<thread_id>.jsonl` inside the
//! project (committed with it, spec §9). The first line is the thread
//! header (`ThreadSummary` plus `"kind":"thread"`); every line after is a
//! `ChatRecord`. Every string is passed through `redact::redact` before it
//! touches disk.
//!
//! Thread ids are `YYYYMMDDTHHMMSSmmm-xxxxxxxx` (UTC time to the
//! millisecond, then 8 random hex chars), so a plain string sort is a
//! chronological sort and ids are safe as file names.
//!
//! Writers (`create_thread`, `append`, `set_provider_session`) serialize on
//! one in-process lock: `set_provider_session` replaces the file by rename,
//! and an `append` racing with it would otherwise write into the old,
//! about-to-be-replaced inode and be lost. InfinaBox is the only process
//! that writes these files.

use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::agent::AgentEvent;
use crate::redact::redact;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ThreadSummary {
    pub id: String,
    pub title: String,
    /// Unix seconds.
    pub created_at: i64,
    pub provider: String,
    pub provider_session_id: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChatRecord {
    User { text: String, at: i64 },
    Event { event: AgentEvent, at: i64 },
}

const HEADER_KIND: &str = "thread";

static WRITE_LOCK: Mutex<()> = Mutex::new(());

/// A poisoned lock only means another writer panicked; the files themselves
/// are still consistent (every write is whole-line or rename-based).
fn write_lock() -> MutexGuard<'static, ()> {
    WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn create_thread(project: &Path, title: &str, provider: &str) -> Result<ThreadSummary> {
    let dir = chat_dir(project);
    let _guard = write_lock();
    fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create chat folder '{}'", dir.display()))?;

    let now = chrono::Utc::now();
    let summary = ThreadSummary {
        id: new_thread_id(now),
        title: title.to_string(),
        created_at: now.timestamp(),
        provider: provider.to_string(),
        provider_session_id: None,
    };
    let header = header_line(&summary)?;

    let path = dir.join(format!("{}.jsonl", summary.id));
    // `create_new`: never clobber an existing thread, however unlikely an
    // id collision is.
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .with_context(|| format!("failed to create chat thread '{}'", path.display()))?;
    file.write_all(header.as_bytes())
        .with_context(|| format!("failed to write chat thread '{}'", path.display()))?;

    // Return what's on disk (i.e. redacted), so callers never hold a
    // version the file doesn't.
    read_header(&path)
}

pub fn append(project: &Path, thread_id: &str, record: &ChatRecord) -> Result<()> {
    let path = thread_path(project, thread_id)?;
    let mut line = redacted_json(record)?;
    line.push('\n');

    let _guard = write_lock();
    if !path.is_file() {
        anyhow::bail!("chat thread '{thread_id}' does not exist");
    }
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .with_context(|| format!("failed to open chat thread '{}'", path.display()))?;
    let end = drop_torn_tail(&mut file)
        .with_context(|| format!("failed to repair chat thread '{}'", path.display()))?;
    file.seek(SeekFrom::Start(end))?;
    // One `write_all` of a whole line keeps each record intact on disk.
    file.write_all(line.as_bytes())
        .with_context(|| format!("failed to append to chat thread '{}'", path.display()))
}

/// Makes sure the file ends in a newline before a record is appended, so
/// the new record starts on its own line. Returns the length to write at.
///
/// - Ends in `\n` (the normal case): checked by reading one byte.
/// - The text after the last newline is valid JSON (a complete record or
///   header whose newline was lost): the newline is added, nothing dropped.
/// - Otherwise it's a fragment from a crash mid-append: the file is
///   truncated back to just after the last newline. It never truncates into
///   the header line; a file with no complete line at all is refused.
fn drop_torn_tail(file: &mut File) -> std::io::Result<u64> {
    let len = file.metadata()?.len();
    if len == 0 {
        return Ok(0);
    }
    let mut last = [0u8; 1];
    file.seek(SeekFrom::Start(len - 1))?;
    file.read_exact(&mut last)?;
    if last[0] == b'\n' {
        return Ok(len);
    }

    let tail_start = last_newline_before(file, len)?.map_or(0, |i| i + 1);
    let mut tail = Vec::with_capacity((len - tail_start) as usize);
    file.seek(SeekFrom::Start(tail_start))?;
    file.read_to_end(&mut tail)?;
    if serde_json::from_slice::<Value>(&tail).is_ok() {
        file.seek(SeekFrom::Start(len))?;
        file.write_all(b"\n")?;
        return Ok(len + 1);
    }
    if tail_start == 0 {
        // The only line is the header, and it's damaged.
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "the thread header line is incomplete",
        ));
    }
    file.set_len(tail_start)?;
    Ok(tail_start)
}

/// Offset of the last `\n` in the first `len` bytes, scanning backwards in
/// chunks so a long thread isn't read whole.
fn last_newline_before(file: &mut File, len: u64) -> std::io::Result<Option<u64>> {
    const CHUNK: u64 = 8192;
    let mut buf = vec![0u8; CHUNK as usize];
    let mut end = len;
    while end > 0 {
        let start = end.saturating_sub(CHUNK);
        let chunk = &mut buf[..(end - start) as usize];
        file.seek(SeekFrom::Start(start))?;
        file.read_exact(chunk)?;
        if let Some(i) = chunk.iter().rposition(|&b| b == b'\n') {
            return Ok(Some(start + i as u64));
        }
        end = start;
    }
    Ok(None)
}

pub fn set_provider_session(project: &Path, thread_id: &str, session_id: &str) -> Result<()> {
    let path = thread_path(project, thread_id)?;
    let _guard = write_lock();
    remove_stale_temp_files(&path);

    let content = fs::read_to_string(&path)
        .with_context(|| format!("failed to read chat thread '{}'", path.display()))?;
    let (first, rest) = match content.split_once('\n') {
        Some((first, rest)) => (first, rest),
        None => (content.as_str(), ""),
    };
    let mut summary = parse_header(first)
        .with_context(|| format!("chat thread '{}' has an invalid header", path.display()))?;
    summary.provider_session_id = Some(session_id.to_string());

    let mut new_content = header_line(&summary)?;
    new_content.push_str(rest);

    // Atomic replace: write a uniquely named sibling temp file, then rename
    // over the original, so a crash leaves either the old or the new file —
    // never a truncated one. The name ends in `.tmp`, which keeps it out of
    // `list_threads` and out of snapshots (the snapshot module ignores
    // `*.tmp`), and a leftover from a crash is removed on the next rewrite.
    let tmp = temp_path(&path);
    let written = (|| -> Result<()> {
        let mut file =
            File::create(&tmp).with_context(|| format!("failed to create '{}'", tmp.display()))?;
        file.write_all(new_content.as_bytes())
            .with_context(|| format!("failed to write '{}'", tmp.display()))?;
        file.sync_all()
            .with_context(|| format!("failed to flush '{}'", tmp.display()))?;
        fs::rename(&tmp, &path)
            .with_context(|| format!("failed to replace chat thread '{}'", path.display()))
    })();
    if written.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    written
}

/// `<id>.jsonl.<random>.tmp`, next to the thread file.
fn temp_path(thread_file: &Path) -> PathBuf {
    let name = thread_file
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    thread_file.with_file_name(format!("{name}.{}.tmp", &suffix[..8]))
}

/// Deletes temp files a crashed rewrite of this thread left behind. Only
/// called while holding the write lock, so none of them is in use.
fn remove_stale_temp_files(thread_file: &Path) {
    let (Some(dir), Some(name)) = (thread_file.parent(), thread_file.file_name()) else {
        return;
    };
    let prefix = format!("{}.", name.to_string_lossy());
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let file_name = entry.file_name().to_string_lossy().into_owned();
        if file_name.starts_with(&prefix) && file_name.ends_with(".tmp") {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// Newest first. Files whose header can't be read are skipped rather than
/// failing the whole list (one damaged thread shouldn't hide the others).
pub fn list_threads(project: &Path) -> Result<Vec<ThreadSummary>> {
    let dir = chat_dir(project);
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => {
            return Err(e)
                .with_context(|| format!("failed to read chat folder '{}'", dir.display()));
        }
    };

    let mut threads = Vec::new();
    for entry in entries {
        let path = entry
            .with_context(|| format!("failed to read chat folder '{}'", dir.display()))?
            .path();
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        if let Ok(summary) = read_header(&path) {
            threads.push(summary);
        }
    }
    threads.sort_by(|a, b| {
        b.created_at
            .cmp(&a.created_at)
            .then_with(|| b.id.cmp(&a.id))
    });
    Ok(threads)
}

pub fn load_thread(project: &Path, thread_id: &str) -> Result<(ThreadSummary, Vec<ChatRecord>)> {
    let path = thread_path(project, thread_id)?;
    let content = fs::read_to_string(&path)
        .with_context(|| format!("failed to read chat thread '{}'", path.display()))?;
    let mut lines = content
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
        .peekable();

    let (_, first) = lines
        .next()
        .with_context(|| format!("chat thread '{}' is empty", path.display()))?;
    let summary = parse_header(first)
        .with_context(|| format!("chat thread '{}' has an invalid header", path.display()))?;

    let mut records = Vec::new();
    while let Some((n, line)) = lines.next() {
        match serde_json::from_str::<ChatRecord>(line) {
            Ok(record) => records.push(record),
            // A crash mid-append can leave a partial final line; drop just
            // that one. Anything malformed earlier is real damage.
            Err(_) if lines.peek().is_none() && !content.ends_with('\n') => {}
            Err(e) => {
                return Err(e).with_context(|| {
                    format!(
                        "chat thread '{}' line {} is not a valid record",
                        path.display(),
                        n + 1
                    )
                });
            }
        }
    }
    Ok((summary, records))
}

fn chat_dir(project: &Path) -> PathBuf {
    project.join(".ibproject").join("chat")
}

/// Rejects anything that isn't a plain id, so a thread id can never point
/// outside `.ibproject/chat/`.
fn thread_path(project: &Path, thread_id: &str) -> Result<PathBuf> {
    let valid = !thread_id.is_empty()
        && thread_id.len() <= 128
        && thread_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !valid {
        anyhow::bail!("invalid chat thread id '{thread_id}'");
    }
    Ok(chat_dir(project).join(format!("{thread_id}.jsonl")))
}

fn new_thread_id(now: chrono::DateTime<chrono::Utc>) -> String {
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    format!("{}-{}", now.format("%Y%m%dT%H%M%S%3f"), &suffix[..8])
}

fn header_line(summary: &ThreadSummary) -> Result<String> {
    let mut value = serde_json::to_value(summary).context("failed to serialize thread header")?;
    redact_strings(&mut value);
    if let Value::Object(map) = &mut value {
        map.insert("kind".to_string(), Value::String(HEADER_KIND.to_string()));
    }
    let mut line = serde_json::to_string(&value).context("failed to serialize thread header")?;
    line.push('\n');
    Ok(line)
}

fn parse_header(line: &str) -> Result<ThreadSummary> {
    let value: Value = serde_json::from_str(line).context("header is not JSON")?;
    if value.get("kind").and_then(Value::as_str) != Some(HEADER_KIND) {
        anyhow::bail!("first line is not a thread header");
    }
    serde_json::from_value(value).context("header is not a valid thread summary")
}

fn read_header(path: &Path) -> Result<ThreadSummary> {
    let file = File::open(path).with_context(|| format!("failed to open '{}'", path.display()))?;
    let mut first = String::new();
    BufReader::new(file)
        .read_line(&mut first)
        .with_context(|| format!("failed to read '{}'", path.display()))?;
    parse_header(first.trim_end())
}

/// Serializes `record` with every string value in it redacted. Tags
/// (`kind`/`type`) and ids pass through unchanged because redaction only
/// touches credential-shaped text.
fn redacted_json(record: &ChatRecord) -> Result<String> {
    let mut value = serde_json::to_value(record).context("failed to serialize chat record")?;
    redact_strings(&mut value);
    serde_json::to_string(&value).context("failed to serialize chat record")
}

fn redact_strings(value: &mut Value) {
    match value {
        Value::String(s) => {
            let redacted = redact(s);
            if redacted != *s {
                *s = redacted;
            }
        }
        Value::Array(items) => items.iter_mut().for_each(redact_strings),
        Value::Object(map) => map.values_mut().for_each(redact_strings),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{AgentErrorKind, Usage};
    use tempfile::TempDir;

    fn sample_records() -> Vec<ChatRecord> {
        vec![
            ChatRecord::User {
                text: "Make the player jump higher".into(),
                at: 100,
            },
            ChatRecord::Event {
                event: AgentEvent::SessionStarted {
                    provider_session_id: "7f9c2ba4-e88f-11ec-8ea0-0242ac120002".into(),
                    model: Some("claude-sonnet".into()),
                },
                at: 101,
            },
            ChatRecord::Event {
                event: AgentEvent::ToolUse {
                    id: "toolu_01ABCdefGHIjklMNOpqrSTU".into(),
                    name: "Edit".into(),
                    summary: "scripts/player.gd".into(),
                },
                at: 102,
            },
            ChatRecord::Event {
                event: AgentEvent::FilesChanged {
                    paths: vec!["scripts/player.gd".into()],
                },
                at: 103,
            },
            ChatRecord::Event {
                event: AgentEvent::AssistantText {
                    text: "Done: `var jump_velocity = -500.0`".into(),
                },
                at: 104,
            },
            ChatRecord::Event {
                event: AgentEvent::TurnCompleted {
                    is_error: false,
                    duration_ms: Some(4200),
                    usage: Some(Usage {
                        input_tokens: Some(1200),
                        output_tokens: None,
                    }),
                },
                at: 105,
            },
            ChatRecord::Event {
                event: AgentEvent::Error {
                    kind: AgentErrorKind::RateLimited,
                    message: "slow down".into(),
                },
                at: 106,
            },
        ]
    }

    #[test]
    fn chat_round_trip() {
        let dir = TempDir::new().unwrap();
        let thread = create_thread(dir.path(), "Jumping", "claude-code").unwrap();
        assert_eq!(thread.title, "Jumping");
        assert_eq!(thread.provider, "claude-code");
        assert_eq!(thread.provider_session_id, None);
        assert!(
            dir.path()
                .join(format!(".ibproject/chat/{}.jsonl", thread.id))
                .is_file()
        );

        let records = sample_records();
        for r in &records {
            append(dir.path(), &thread.id, r).unwrap();
        }
        let (loaded, loaded_records) = load_thread(dir.path(), &thread.id).unwrap();
        assert_eq!(loaded, thread);
        assert_eq!(loaded_records, records);

        // File layout: header first with kind=thread, then one record per line.
        let raw = fs::read_to_string(
            dir.path()
                .join(format!(".ibproject/chat/{}.jsonl", thread.id)),
        )
        .unwrap();
        let lines: Vec<&str> = raw.lines().collect();
        assert_eq!(lines.len(), 1 + records.len());
        let header: Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(header["kind"], "thread");
        assert_eq!(header["id"], thread.id.as_str());
        let first: Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(first["kind"], "user");
    }

    #[test]
    fn redaction_is_applied_on_write() {
        let dir = TempDir::new().unwrap();
        let key = "sk-ant-api03-AbCdEfGhIjKlMnOpQrStUvWxYz0123456789";
        let thread =
            create_thread(dir.path(), &format!("Setup with {key}"), "claude-code").unwrap();
        assert_eq!(thread.title, "Setup with [redacted]");

        append(
            dir.path(),
            &thread.id,
            &ChatRecord::User {
                text: format!("here is my key {key}"),
                at: 1,
            },
        )
        .unwrap();
        append(
            dir.path(),
            &thread.id,
            &ChatRecord::Event {
                event: AgentEvent::ToolResult {
                    id: "toolu_1".into(),
                    ok: true,
                    summary: "wrote .env: STRIPE_SECRET_KEY=sk_live_51Habcdefghijklmnop".into(),
                },
                at: 2,
            },
        )
        .unwrap();

        let raw = fs::read_to_string(
            dir.path()
                .join(format!(".ibproject/chat/{}.jsonl", thread.id)),
        )
        .unwrap();
        assert!(!raw.contains(key));
        assert!(!raw.contains("sk_live_51Habcdefghijklmnop"));

        let (_, records) = load_thread(dir.path(), &thread.id).unwrap();
        assert_eq!(
            records[0],
            ChatRecord::User {
                text: "here is my key [redacted]".into(),
                at: 1
            }
        );
        match &records[1] {
            ChatRecord::Event {
                event: AgentEvent::ToolResult { summary, id, .. },
                ..
            } => {
                assert_eq!(summary, "wrote .env: STRIPE_SECRET_KEY=[redacted]");
                assert_eq!(id, "toolu_1");
            }
            other => panic!("unexpected record {other:?}"),
        }
    }

    #[test]
    fn header_rewrite_sets_provider_session_and_keeps_records() {
        let dir = TempDir::new().unwrap();
        let thread = create_thread(dir.path(), "Level two", "claude-code").unwrap();
        let records = sample_records();
        for r in &records {
            append(dir.path(), &thread.id, r).unwrap();
        }

        set_provider_session(dir.path(), &thread.id, "session-abc-123").unwrap();
        let (loaded, loaded_records) = load_thread(dir.path(), &thread.id).unwrap();
        assert_eq!(
            loaded.provider_session_id.as_deref(),
            Some("session-abc-123")
        );
        assert_eq!(
            ThreadSummary {
                provider_session_id: None,
                ..loaded
            },
            thread
        );
        assert_eq!(loaded_records, records);

        // Rewriting again replaces it; no temp file is left behind.
        set_provider_session(dir.path(), &thread.id, "session-def-456").unwrap();
        let (loaded, _) = load_thread(dir.path(), &thread.id).unwrap();
        assert_eq!(
            loaded.provider_session_id.as_deref(),
            Some("session-def-456")
        );
        let names: Vec<_> = fs::read_dir(dir.path().join(".ibproject/chat"))
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(names, [format!("{}.jsonl", thread.id)]);

        // Appends after the rewrite still land after the records.
        append(
            dir.path(),
            &thread.id,
            &ChatRecord::User {
                text: "more".into(),
                at: 9,
            },
        )
        .unwrap();
        let (_, loaded_records) = load_thread(dir.path(), &thread.id).unwrap();
        assert_eq!(loaded_records.len(), records.len() + 1);
    }

    #[test]
    fn list_threads_is_newest_first_and_ids_sort_chronologically() {
        let dir = TempDir::new().unwrap();
        assert!(list_threads(dir.path()).unwrap().is_empty());

        let a = create_thread(dir.path(), "A", "claude-code").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        let b = create_thread(dir.path(), "B", "claude-code").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        let c = create_thread(dir.path(), "C", "claude-code").unwrap();
        assert!(a.id < b.id && b.id < c.id, "{} {} {}", a.id, b.id, c.id);

        // Id format: YYYYMMDDTHHMMSSmmm-xxxxxxxx
        let (stamp, suffix) = a.id.split_once('-').unwrap();
        assert_eq!(stamp.len(), 18);
        assert_eq!(&stamp[8..9], "T");
        assert_eq!(suffix.len(), 8);
        assert!(suffix.chars().all(|ch| ch.is_ascii_hexdigit()));

        // Junk in the folder is ignored.
        fs::write(dir.path().join(".ibproject/chat/notes.txt"), "hi").unwrap();
        fs::write(
            dir.path().join(".ibproject/chat/broken.jsonl"),
            "not json\n",
        )
        .unwrap();

        let titles: Vec<_> = list_threads(dir.path())
            .unwrap()
            .into_iter()
            .map(|t| t.title)
            .collect();
        assert_eq!(titles, ["C", "B", "A"]);
    }

    #[test]
    fn rejects_bad_ids_and_missing_threads() {
        let dir = TempDir::new().unwrap();
        let rec = ChatRecord::User {
            text: "x".into(),
            at: 0,
        };
        assert!(append(dir.path(), "../../etc/passwd", &rec).is_err());
        assert!(load_thread(dir.path(), "a/b").is_err());
        assert!(append(dir.path(), "20260101T000000000-deadbeef", &rec).is_err());
        assert!(set_provider_session(dir.path(), "20260101T000000000-deadbeef", "s").is_err());
    }

    #[test]
    fn load_tolerates_only_a_torn_final_line() {
        let dir = TempDir::new().unwrap();
        let thread = create_thread(dir.path(), "T", "claude-code").unwrap();
        append(
            dir.path(),
            &thread.id,
            &ChatRecord::User {
                text: "ok".into(),
                at: 1,
            },
        )
        .unwrap();
        let path = dir
            .path()
            .join(format!(".ibproject/chat/{}.jsonl", thread.id));

        let mut f = OpenOptions::new().append(true).open(&path).unwrap();
        f.write_all(br#"{"kind":"user","te"#).unwrap();
        drop(f);
        let (_, records) = load_thread(dir.path(), &thread.id).unwrap();
        assert_eq!(records.len(), 1);

        // Damage in the middle is reported, not silently dropped.
        let raw = fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = raw.lines().collect();
        fs::write(&path, format!("{}\ngarbage\n{}\n", lines[0], lines[1])).unwrap();
        assert!(load_thread(dir.path(), &thread.id).is_err());
    }

    #[test]
    fn append_after_a_torn_line_drops_the_fragment_and_stays_loadable() {
        let dir = TempDir::new().unwrap();
        let thread = create_thread(dir.path(), "T", "claude-code").unwrap();
        let first = ChatRecord::User {
            text: "one".into(),
            at: 1,
        };
        let second = ChatRecord::User {
            text: "two".into(),
            at: 2,
        };
        append(dir.path(), &thread.id, &first).unwrap();
        let path = dir
            .path()
            .join(format!(".ibproject/chat/{}.jsonl", thread.id));

        let mut f = OpenOptions::new().append(true).open(&path).unwrap();
        f.write_all(br#"{"kind":"user","te"#).unwrap();
        drop(f);

        append(dir.path(), &thread.id, &second).unwrap();
        let (_, records) = load_thread(dir.path(), &thread.id).unwrap();
        assert_eq!(records, vec![first, second]);
        assert!(fs::read_to_string(&path).unwrap().ends_with("\n"));
    }

    #[test]
    fn concurrent_appends_and_header_rewrites_lose_nothing() {
        let dir = TempDir::new().unwrap();
        let project = dir.path().to_path_buf();
        let thread = create_thread(&project, "Busy", "claude-code").unwrap();

        const N: i64 = 200;
        let appender = {
            let (project, id) = (project.clone(), thread.id.clone());
            std::thread::spawn(move || {
                for i in 0..N {
                    append(
                        &project,
                        &id,
                        &ChatRecord::User {
                            text: format!("m{i}"),
                            at: i,
                        },
                    )
                    .unwrap();
                }
            })
        };
        let rewriter = {
            let (project, id) = (project.clone(), thread.id.clone());
            std::thread::spawn(move || {
                for i in 0..N {
                    set_provider_session(&project, &id, &format!("s{i}")).unwrap();
                }
            })
        };
        appender.join().unwrap();
        rewriter.join().unwrap();

        let (summary, records) = load_thread(&project, &thread.id).unwrap();
        assert_eq!(summary.provider_session_id.as_deref(), Some("s199"));
        let ats: Vec<i64> = records
            .iter()
            .map(|r| match r {
                ChatRecord::User { at, .. } => *at,
                other => panic!("unexpected {other:?}"),
            })
            .collect();
        assert_eq!(ats, (0..N).collect::<Vec<_>>());
    }

    #[test]
    fn stale_temp_files_are_cleaned_up_on_rewrite() {
        let dir = TempDir::new().unwrap();
        let thread = create_thread(dir.path(), "T", "claude-code").unwrap();
        let chat = dir.path().join(".ibproject/chat");
        let stale = chat.join(format!("{}.jsonl.deadbeef.tmp", thread.id));
        let other = chat.join("20200101T000000000-00000000.jsonl.cafebabe.tmp");
        fs::write(&stale, "leftover").unwrap();
        fs::write(&other, "someone else's").unwrap();

        set_provider_session(dir.path(), &thread.id, "s").unwrap();
        assert!(!stale.exists());
        assert!(other.exists(), "only this thread's temp files are touched");
        assert_eq!(list_threads(dir.path()).unwrap().len(), 1);
    }

    fn thread_file(dir: &Path, id: &str) -> PathBuf {
        dir.join(format!(".ibproject/chat/{id}.jsonl"))
    }

    #[test]
    fn a_complete_last_line_missing_its_newline_is_kept() {
        let dir = TempDir::new().unwrap();
        let thread = create_thread(dir.path(), "T", "claude-code").unwrap();
        let first = ChatRecord::User {
            text: "one".into(),
            at: 1,
        };
        let second = ChatRecord::User {
            text: "two".into(),
            at: 2,
        };
        append(dir.path(), &thread.id, &first).unwrap();
        let path = thread_file(dir.path(), &thread.id);
        let raw = fs::read_to_string(&path).unwrap();
        fs::write(&path, raw.trim_end_matches('\n')).unwrap();

        append(dir.path(), &thread.id, &second).unwrap();
        let (_, records) = load_thread(dir.path(), &thread.id).unwrap();
        assert_eq!(records, vec![first, second]);
    }

    #[test]
    fn the_header_is_never_truncated() {
        let dir = TempDir::new().unwrap();
        let thread = create_thread(dir.path(), "T", "claude-code").unwrap();
        let path = thread_file(dir.path(), &thread.id);
        let header = fs::read_to_string(&path).unwrap();

        // Header alone, newline lost: repaired, not wiped.
        fs::write(&path, header.trim_end_matches('\n')).unwrap();
        let rec = ChatRecord::User {
            text: "hi".into(),
            at: 1,
        };
        append(dir.path(), &thread.id, &rec).unwrap();
        let (summary, records) = load_thread(dir.path(), &thread.id).unwrap();
        assert_eq!(summary, thread);
        assert_eq!(records, vec![rec.clone()]);

        // A torn header with no complete line: refused, file left as is.
        let torn = &header[..header.len() / 2];
        fs::write(&path, torn).unwrap();
        assert!(append(dir.path(), &thread.id, &rec).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), torn);
    }

    #[test]
    fn torn_tail_repair_scans_back_across_chunks() {
        let dir = TempDir::new().unwrap();
        let thread = create_thread(dir.path(), "T", "claude-code").unwrap();
        let big = ChatRecord::User {
            text: "x".repeat(20_000),
            at: 1,
        };
        append(dir.path(), &thread.id, &big).unwrap();
        let path = thread_file(dir.path(), &thread.id);
        // A fragment longer than one 8 KiB scan chunk.
        let mut f = OpenOptions::new().append(true).open(&path).unwrap();
        f.write_all(format!(r#"{{"kind":"user","text":"{}"#, "y".repeat(20_000)).as_bytes())
            .unwrap();
        drop(f);

        let small = ChatRecord::User {
            text: "after".into(),
            at: 2,
        };
        append(dir.path(), &thread.id, &small).unwrap();
        let (_, records) = load_thread(dir.path(), &thread.id).unwrap();
        assert_eq!(records, vec![big, small]);
    }

    #[test]
    fn appending_to_a_long_thread_does_not_reread_it() {
        // Guards against reading the whole file on every append (which made
        // 2000 × 20 KB appends take ~18 s). Build a ~40 MB thread directly,
        // then append small records: reading it whole each time would move
        // ~20 GB; checking just the tail is instant.
        let dir = TempDir::new().unwrap();
        let thread = create_thread(dir.path(), "T", "claude-code").unwrap();
        let line = format!(
            "{}\n",
            serde_json::to_string(&ChatRecord::User {
                text: "z".repeat(20_000),
                at: 0
            })
            .unwrap()
        );
        let mut f = OpenOptions::new()
            .append(true)
            .open(thread_file(dir.path(), &thread.id))
            .unwrap();
        for _ in 0..2000 {
            f.write_all(line.as_bytes()).unwrap();
        }
        drop(f);

        let started = std::time::Instant::now();
        for i in 0..500 {
            append(
                dir.path(),
                &thread.id,
                &ChatRecord::User {
                    text: "hi".into(),
                    at: i,
                },
            )
            .unwrap();
        }
        let elapsed = started.elapsed();
        assert!(elapsed.as_secs() < 5, "500 appends took {elapsed:?}");
    }
}
