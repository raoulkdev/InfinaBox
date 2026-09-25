//! Replaces anything that looks like a credential with `[redacted]` before
//! chat text is written into the (committed) project. Errs on the side of
//! redacting. Phase A Task D fills this in.

pub fn redact(text: &str) -> String {
    text.to_string()
}
