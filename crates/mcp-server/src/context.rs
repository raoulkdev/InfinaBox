//! Context cards on disk: the plain-filesystem half of the MCP server's
//! tools. Everything is rooted at `<project>/.ibproject/context/` and no
//! path the agent passes in may reach outside it.
//!
//! Plain synchronous code with no MCP types, so it's tested directly
//! against a temp project (the same split as `terminal.rs`/`watcher.rs` in
//! the app: testable logic here, thin tool wrappers in `server.rs`).

use std::fs;
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
        let full = checked_join(&root, rel)?;
        let real = full
            .canonicalize()
            .with_context(|| format!("no context card at {rel}"))?;
        // Catches symlinks inside the folder that point outside it.
        if !real.starts_with(&root) {
            bail!("refusing {rel}: it resolves outside the context folder");
        }
        fs::read_to_string(&real).with_context(|| format!("couldn't read {rel} as text"))
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
        if rel_path.extension().and_then(|e| e.to_str()) != Some(CARD_EXTENSION) {
            bail!("context cards are markdown files; use a path ending in .{CARD_EXTENSION}");
        }
        fs::create_dir_all(&self.root)
            .with_context(|| format!("couldn't create {}", self.root.display()))?;
        let root = self.root.canonicalize()?;

        let target = root.join(&rel_path);
        let parent = target
            .parent()
            .ok_or_else(|| anyhow!("refusing {rel}: no parent folder"))?;
        // `checked_relative` already rejected `..` and absolute paths, so
        // this can only create folders under the root, unless an existing
        // folder along the way is a symlink out of it: canonicalizing the
        // parent below catches that before anything is written.
        fs::create_dir_all(parent)?;
        let real_parent = parent.canonicalize()?;
        if !real_parent.starts_with(&root) {
            bail!("refusing {rel}: it resolves outside the context folder");
        }
        let file_name = target
            .file_name()
            .expect("has an .md extension, so a file name");
        let real_target = real_parent.join(file_name);
        if let Ok(meta) = fs::symlink_metadata(&real_target) {
            if meta.file_type().is_symlink() {
                bail!("refusing {rel}: it is a symlink");
            }
            if meta.is_dir() {
                bail!("refusing {rel}: it is a folder");
            }
        }
        fs::write(&real_target, markdown).with_context(|| format!("couldn't write {rel}"))?;
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
/// no drive prefixes, no `..`. The on-disk (symlink) half of the check
/// happens after canonicalizing, in the callers.
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

fn checked_join(root: &Path, rel: &str) -> Result<PathBuf> {
    Ok(root.join(checked_relative(rel)?))
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
