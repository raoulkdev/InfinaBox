//! Sandboxed file tools for the API runtimes: `read_file`, `write_file`,
//! `edit_file` (exact-string replace), `list_files`, `search_files`. Paths
//! are project-relative and must stay inside the project; writes to `.git`,
//! `addons/infinabox`, `.ibproject/.ibx` and `.ibproject/chat` are refused.
//! Wave 0 stub — task RL.
