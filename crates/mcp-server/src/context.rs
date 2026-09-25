//! Context cards on disk: the plain-filesystem half of the MCP server's
//! tools. Everything is rooted at `<project>/.ibproject/context/` and no
//! path the agent passes in may reach outside it.
//!
//! Plain synchronous code with no MCP types, so it's tested directly
//! against a temp project (the same split as `terminal.rs`/`watcher.rs` in
//! the app: testable logic here, thin tool wrappers in `server.rs`).

use std::ffi::OsStr;
use std::fs;
use std::io::{Read as _, Write as _};
use std::path::{Component, Path, PathBuf};

use anyhow::{Context as _, Result, anyhow, bail};
use serde::Serialize;

/// Where Context lives inside a project (spec §9).
pub const CONTEXT_DIR: &[&str] = &[".ibproject", "context"];

/// Cards are markdown files (spec §9: `*.md` with YAML front-matter).
const CARD_EXTENSION: &str = "md";

/// One matching line from `search`.
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct SearchHit {
    /// Card path relative to the context folder, `/`-separated.
    pub path: String,
    /// 1-based line number.
    pub line: usize,
    pub text: String,
}

/// A project's Context folder. Doesn't need to exist yet: listing and
/// searching a missing folder is simply empty, and writing creates it.
pub struct ContextDir {
    root: PathBuf,
}

impl ContextDir {
    pub fn for_project(project: &Path) -> Self {
        let mut root = project.to_path_buf();
        for part in CONTEXT_DIR {
            root.push(part);
        }
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Every card, as `/`-separated paths relative to the context folder,
    /// sorted. Hidden files and folders are skipped.
    pub fn list(&self) -> Result<Vec<String>> {
        let Some(root) = self.canonical_root()? else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        collect_cards(&root, &root, &mut out)?;
        out.sort();
        Ok(out)
    }

    /// The full text of one card (or any file inside the context folder).
    pub fn read(&self, rel: &str) -> Result<String> {
        let root = self
            .canonical_root()?
            .ok_or_else(|| anyhow!("this project has no Context folder yet"))?;
        let rel_path = checked_relative(rel)?;
        let (dirs, file_name) = split_file(&rel_path, rel)?;
        let parent = resolve_dir(&root, &dirs, false, rel)?;
        let path = parent.join(file_name);

        // Check the entry itself without following it, then open it and make
        // sure the handle we got is that same regular file. A symlink swapped
        // in between the check and the open fails the identity check.
        let before =
            fs::symlink_metadata(&path).with_context(|| format!("no context card at {rel}"))?;
        if before.file_type().is_symlink() {
            bail!("refusing {rel}: it is a symlink");
        }
        if !before.is_file() {
            bail!("refusing {rel}: not a file");
        }
        let mut file = fs::File::open(&path).with_context(|| format!("couldn't open {rel}"))?;
        if !same_file(&before, &file.metadata()?) {
            bail!("refusing {rel}: it changed while being opened");
        }
        let mut text = String::new();
        file.read_to_string(&mut text)
            .with_context(|| format!("couldn't read {rel} as text"))?;
        Ok(text)
    }

    /// Case-insensitive plain-text search across every card's lines.
    pub fn search(&self, query: &str) -> Result<Vec<SearchHit>> {
        let needle = query.trim().to_lowercase();
        if needle.is_empty() {
            bail!("search query is empty");
        }
        let mut hits = Vec::new();
        for rel in self.list()? {
            // A card that isn't valid UTF-8 can't match a text query; skip it
            // rather than failing the whole search.
            let Ok(text) = self.read(&rel) else { continue };
            for (i, line) in text.lines().enumerate() {
                if line.to_lowercase().contains(&needle) {
                    hits.push(SearchHit {
                        path: rel.clone(),
                        line: i + 1,
                        text: line.to_string(),
                    });
                }
            }
        }
        Ok(hits)
    }

    /// Creates or replaces a card, creating the context folder and any
    /// subfolders as needed. Returns the normalized relative path written.
    pub fn write(&self, rel: &str, markdown: &str) -> Result<String> {
        let rel_path = checked_relative(rel)?;
        check_card_components(&rel_path, rel)?;
        if rel_path.extension().and_then(|e| e.to_str()) != Some(CARD_EXTENSION) {
            bail!("context cards are markdown files; use a path ending in .{CARD_EXTENSION}");
        }
        fs::create_dir_all(&self.root)
            .with_context(|| format!("couldn't create {}", self.root.display()))?;
        let root = self.root.canonicalize()?;

        // Subfolders are created one level at a time from the canonical root,
        // each existing level verified to be a real folder (not a symlink)
        // first, so nothing is ever created outside the context folder.
        let (dirs, file_name) = split_file(&rel_path, rel)?;
        let parent = resolve_dir(&root, &dirs, true, rel)?;
        let target = parent.join(file_name);
        if let Ok(meta) = fs::symlink_metadata(&target) {
            if meta.file_type().is_symlink() {
                bail!("refusing {rel}: it is a symlink");
            }
            if meta.is_dir() {
                bail!("refusing {rel}: it is a folder");
            }
        }

        // Write a fresh temp file beside the target, then rename it over the
        // target. `create_new` won't follow anything already at the temp
        // path, and rename replaces the target entry itself (even one swapped
        // for a symlink after the check above) instead of writing through
        // it. It's also atomic, so a reader never sees half a card.
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let tmp = parent.join(format!(
            ".{}.{}-{nanos}.tmp",
            file_name.to_string_lossy(),
            std::process::id()
        ));
        let written = (|| -> Result<()> {
            let mut f = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&tmp)?;
            f.write_all(markdown.as_bytes())?;
            f.sync_all()?;
            drop(f);
            fs::rename(&tmp, &target)?;
            Ok(())
        })();
        if let Err(e) = written {
            let _ = fs::remove_file(&tmp);
            return Err(e.context(format!("couldn't write {rel}")));
        }
        Ok(to_slash(&rel_path))
    }

    fn canonical_root(&self) -> Result<Option<PathBuf>> {
        match self.root.canonicalize() {
            Ok(p) => Ok(Some(p)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e).with_context(|| format!("couldn't open {}", self.root.display())),
        }
    }
}

/// Validates an agent-supplied relative path lexically: no absolute paths,
/// no drive prefixes, no `..`. The on-disk (symlink) half of the check is
/// `resolve_dir` plus the per-file checks in the callers.
fn checked_relative(rel: &str) -> Result<PathBuf> {
    let rel = rel.trim();
    if rel.is_empty() {
        bail!("path is empty");
    }
    let mut out = PathBuf::new();
    for comp in Path::new(rel).components() {
        match comp {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                bail!("refusing {rel}: paths must stay inside the context folder")
            }
        }
    }
    if out.as_os_str().is_empty() {
        bail!("refusing {rel}: not a file path");
    }
    Ok(out)
}

/// Written cards must be listable and portable: no hidden components (they
/// would never show up in `list`), and no `\` or `:` (a separator or an
/// invalid name character on some platforms).
fn check_card_components(rel_path: &Path, rel: &str) -> Result<()> {
    for comp in rel_path.components() {
        let part = comp
            .as_os_str()
            .to_str()
            .ok_or_else(|| anyhow!("refusing {rel}: not valid UTF-8"))?;
        if part.starts_with('.') {
            bail!("refusing {rel}: names starting with '.' are hidden and not allowed");
        }
        if part.contains('\\') || part.contains(':') {
            bail!("refusing {rel}: use '/' between folders, and no ':' in names");
        }
    }
    Ok(())
}

/// Splits a checked relative path into its folder components and file name.
fn split_file<'a>(rel_path: &'a Path, rel: &str) -> Result<(Vec<&'a OsStr>, &'a OsStr)> {
    let mut parts: Vec<&OsStr> = rel_path.components().map(|c| c.as_os_str()).collect();
    let file = parts
        .pop()
        .ok_or_else(|| anyhow!("refusing {rel}: not a file path"))?;
    Ok((parts, file))
}

/// Walks `dirs` down from the canonical `root` one level at a time. Every
/// existing level must be a real folder, never a symlink. With `create`, a
/// missing level is made with a single `create_dir`, which fails rather than
/// following anything that appears there in the meantime. Returns the
/// canonical folder reached.
fn resolve_dir(root: &Path, dirs: &[&OsStr], create: bool, rel: &str) -> Result<PathBuf> {
    let mut cur = root.to_path_buf();
    for part in dirs {
        cur.push(part);
        match fs::symlink_metadata(&cur) {
            Ok(meta) if meta.is_dir() => {}
            Ok(meta) if meta.file_type().is_symlink() => {
                bail!("refusing {rel}: a folder on the way is a symlink")
            }
            Ok(_) => bail!("refusing {rel}: a folder on the way is a file"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && create => {
                fs::create_dir(&cur)
                    .with_context(|| format!("couldn't create a folder for {rel}"))?;
                if !fs::symlink_metadata(&cur)?.is_dir() {
                    bail!("refusing {rel}: a folder on the way changed while being created");
                }
            }
            Err(e) => return Err(e).with_context(|| format!("no context card at {rel}")),
        }
    }
    // Belt and braces: the folder reached really is inside the root.
    let real = cur.canonicalize()?;
    if !real.starts_with(root) {
        bail!("refusing {rel}: it resolves outside the context folder");
    }
    Ok(real)
}

#[cfg(unix)]
fn same_file(a: &fs::Metadata, b: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    a.dev() == b.dev() && a.ino() == b.ino()
}

/// Stable std has no file identity on other platforms; fall back to checking
/// the opened handle is a regular file (the pre-open check refused symlinks).
#[cfg(not(unix))]
fn same_file(_a: &fs::Metadata, b: &fs::Metadata) -> bool {
    b.is_file()
}

fn collect_cards(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        // `file_type` doesn't follow symlinks, so a symlinked folder can't
        // lead the walk outside the context folder.
        let ft = entry.file_type()?;
        let path = entry.path();
        if ft.is_dir() {
            collect_cards(root, &path, out)?;
        } else if ft.is_file() && path.extension().and_then(|e| e.to_str()) == Some(CARD_EXTENSION)
        {
            out.push(to_slash(path.strip_prefix(root).unwrap_or(&path)));
        }
    }
    Ok(())
}

fn to_slash(p: &Path) -> String {
    p.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_project() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "infinabox-mcp-ctx-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn missing_context_folder_lists_empty() {
        let project = temp_project();
        let ctx = ContextDir::for_project(&project);
        assert_eq!(ctx.list().unwrap(), Vec::<String>::new());
        assert!(ctx.read("concept.md").is_err());
        fs::remove_dir_all(project).ok();
    }

    #[test]
    fn write_list_read_search_round_trip() {
        let project = temp_project();
        let ctx = ContextDir::for_project(&project);

        assert_eq!(
            ctx.write("concept.md", "# Concept\nA cozy platformer.\n")
                .unwrap(),
            "concept.md"
        );
        assert_eq!(
            ctx.write(
                "characters/boss.md",
                "# Boss\nThe Boss has a DOUBLE jump.\n"
            )
            .unwrap(),
            "characters/boss.md"
        );
        // Non-card and hidden files are ignored by list and search.
        fs::write(ctx.root().join("notes.txt"), "double").unwrap();
        fs::write(ctx.root().join(".hidden.md"), "double").unwrap();

        assert_eq!(
            ctx.list().unwrap(),
            vec!["characters/boss.md", "concept.md"]
        );
        assert_eq!(
            ctx.read("characters/boss.md").unwrap(),
            "# Boss\nThe Boss has a DOUBLE jump.\n"
        );

        let hits = ctx.search("Double Jump").unwrap();
        assert_eq!(
            hits,
            vec![SearchHit {
                path: "characters/boss.md".into(),
                line: 2,
                text: "The Boss has a DOUBLE jump.".into()
            }]
        );
        assert!(ctx.search("   ").is_err());

        // Overwrite replaces the card.
        ctx.write("concept.md", "new").unwrap();
        assert_eq!(ctx.read("concept.md").unwrap(), "new");

        fs::remove_dir_all(project).ok();
    }

    #[test]
    fn write_requires_markdown() {
        let project = temp_project();
        let ctx = ContextDir::for_project(&project);
        assert!(ctx.write("script.gd", "extends Node").is_err());
        assert!(!project.join("script.gd").exists());
        fs::remove_dir_all(project).ok();
    }

    #[test]
    fn write_refuses_hidden_or_unportable_names() {
        let project = temp_project();
        let ctx = ContextDir::for_project(&project);
        for bad in [
            ".hidden.md",
            ".secret/card.md",
            "a\\b.md",
            "c:d.md",
            "dir:x/card.md",
        ] {
            assert!(ctx.write(bad, "x").is_err(), "write should refuse {bad:?}");
        }
        // Nothing was written, not even an empty folder.
        assert_eq!(ctx.list().unwrap(), Vec::<String>::new());
        if ctx.root().exists() {
            assert_eq!(fs::read_dir(ctx.root()).unwrap().count(), 0);
        }
        // Temp files from the atomic write never linger.
        ctx.write("sub/ok.md", "fine").unwrap();
        assert_eq!(fs::read_dir(ctx.root().join("sub")).unwrap().count(), 1);
        fs::remove_dir_all(project).ok();
    }

    #[test]
    fn refuses_paths_that_escape_the_context_folder() {
        let project = temp_project();
        let ctx = ContextDir::for_project(&project);
        ctx.write("ok.md", "fine").unwrap();
        fs::write(project.join("secret.md"), "outside").unwrap();

        for bad in [
            "../../secret.md",
            "../context/../../secret.md",
            "sub/../../../secret.md",
            "/etc/passwd",
            &project.join("secret.md").to_string_lossy(),
            "",
            ".",
        ] {
            assert!(ctx.read(bad).is_err(), "read should refuse {bad:?}");
            assert!(ctx.write(bad, "x").is_err(), "write should refuse {bad:?}");
        }
        // Nothing outside was touched.
        assert_eq!(
            fs::read_to_string(project.join("secret.md")).unwrap(),
            "outside"
        );
        assert!(!project.join(".ibproject").join("secret.md").exists());

        fs::remove_dir_all(project).ok();
    }

    #[cfg(unix)]
    #[test]
    fn refuses_symlinks_that_point_outside() {
        use std::os::unix::fs::symlink;

        let project = temp_project();
        let outside = temp_project();
        fs::write(outside.join("secret.md"), "outside").unwrap();

        let ctx = ContextDir::for_project(&project);
        ctx.write("ok.md", "fine").unwrap();
        symlink(outside.join("secret.md"), ctx.root().join("link.md")).unwrap();
        symlink(&outside, ctx.root().join("linkdir")).unwrap();

        assert!(ctx.read("link.md").is_err());
        assert!(ctx.read("linkdir/secret.md").is_err());
        assert!(ctx.write("linkdir/secret.md", "pwned").is_err());
        assert!(ctx.write("linkdir/new.md", "pwned").is_err());
        assert!(ctx.write("link.md", "pwned").is_err());
        // Regression: nested folders under a symlinked folder must not be
        // created outside before the write is refused.
        assert!(ctx.write("linkdir/a/b/new.md", "pwned").is_err());
        assert!(!outside.join("a").exists());
        assert!(ctx.read("linkdir/a/b/new.md").is_err());
        assert_eq!(
            fs::read_to_string(outside.join("secret.md")).unwrap(),
            "outside"
        );
        assert!(!outside.join("new.md").exists());
        // Symlinks aren't listed (so search never follows them either).
        assert_eq!(ctx.list().unwrap(), vec!["ok.md"]);

        fs::remove_dir_all(project).ok();
        fs::remove_dir_all(outside).ok();
    }
}
