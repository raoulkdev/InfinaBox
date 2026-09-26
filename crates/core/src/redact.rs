//! Replaces anything that looks like a credential with `[redacted]` before
//! chat text is written into the (committed) project — spec §9: chat threads
//! live in `.ibproject/chat/` and are committed with the game, so a key a
//! user pastes into the chat must never reach disk.
//!
//! Errs on the side of redacting: a false positive costs a few characters of
//! chat history, a false negative leaks a secret into git. The one place it
//! holds back is the generic `name = value` rule, which skips values that are
//! plainly code (numbers, calls, node paths, type annotations) so ordinary
//! GDScript like `var token_count = 3` survives intact.

use std::sync::LazyLock;

use regex::{Captures, Regex};

/// What every detected secret is replaced with.
pub const REDACTED: &str = "[redacted]";

/// Whole-token formats with a recognizable prefix: the entire match is
/// replaced. Order doesn't matter for correctness (each replaces with the
/// same marker, which none of them match), but the most specific come first
/// for readability.
static TOKEN_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        // PEM private key blocks (RSA/EC/OPENSSH/ENCRYPTED/plain) and PGP
        // private key blocks. An unterminated block is redacted to the end
        // of the text rather than left half-exposed.
        r"(?s)-----BEGIN [A-Z0-9 ]*PRIVATE KEY(?: BLOCK)?-----.*?(?:-----END [A-Z0-9 ]*PRIVATE KEY(?: BLOCK)?-----|\z)",
        // Anthropic (`sk-ant-...`) and OpenAI (`sk-proj-...`,
        // `sk-svcacct-...`, `sk-admin-...`, legacy `sk-<48 chars>`) keys.
        // The legacy form requires a long unbroken alphanumeric run so
        // kebab-case names like `sk-level-boss-fight.tscn` survive.
        r"\bsk-(?:ant|proj|svcacct|admin)-[A-Za-z0-9_\-]{16,}",
        r"\bsk-[A-Za-z0-9]{32,}",
        // Stripe secret/restricted/publishable keys and webhook secrets;
        // ElevenLabs-style `sk_<hex>` keys.
        r"\b(?:sk|rk|pk)_(?:live|test)_[A-Za-z0-9]{8,}",
        r"\bwhsec_[A-Za-z0-9]{16,}",
        r"\bsk_[A-Za-z0-9]{20,}",
        // GitHub personal/OAuth/app tokens and fine-grained PATs.
        r"\b(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{20,}",
        r"\bgithub_pat_[A-Za-z0-9_]{20,}",
        // AWS access key ids.
        r"\b(?:AKIA|ASIA|ABIA|ACCA|AGPA|AIDA|AROA|ANPA|ANVA|AIPA)[A-Z0-9]{16}\b",
        // JWT-looking tokens (Supabase anon/service-role keys are JWTs).
        r"\beyJ[A-Za-z0-9_\-]{5,}\.eyJ[A-Za-z0-9_\-]{5,}\.[A-Za-z0-9_\-]{5,}",
        // Supabase's newer non-JWT keys.
        r"\bsb_(?:secret|publishable)_[A-Za-z0-9_\-]{10,}",
        // Google API keys, Slack tokens, npm tokens, Hugging Face tokens.
        r"\bAIza[0-9A-Za-z_\-]{30,}",
        r"\bxox[abposr]-[A-Za-z0-9\-]{10,}",
        r"\bnpm_[A-Za-z0-9]{30,}",
        r"\bhf_[A-Za-z0-9]{30,}",
    ]
    .iter()
    .map(|p| Regex::new(p).expect("redaction pattern must compile"))
    .collect()
});

/// `Authorization: Bearer <value>` (header, JSON, or `curl -H` style). Keeps
/// the header name and scheme, redacts the credential.
static AUTH_HEADER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?i)(?P<pre>\b(?:proxy-)?authorization["']?\s*[:=]\s*["']?\s*(?:(?:bearer|basic|token|bot|digest)\s+)?)(?P<val>[^\s"',;]+)"#,
    )
    .expect("auth header pattern must compile")
});

/// A bare `Bearer <credential>` without the header name in front.
static BEARER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?P<pre>\bbearer\s+)(?P<val>[A-Za-z0-9\-._~+/]{12,}=*)")
        .expect("bearer pattern must compile")
});

/// Credentials embedded in a URL: `scheme://user:password@host`.
static URL_USERINFO: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?P<pre>\b[a-z][a-z0-9+.\-]*://[^\s:/@]+:)(?P<val>[^\s@/]+)(?P<post>@)")
        .expect("url userinfo pattern must compile")
});

/// Generic `name = value` / `name: value` / `"name": "value"` where the name
/// *ends* in a credential word (`api_key`, `ANTHROPIC_API_KEY`,
/// `client_secret`, `access_token`, `password`, ...). Ending-only matters:
/// `secret_door` or `token_count` are game identifiers, not credentials.
///
/// The optional `: Type` before `=` lets a typed GDScript declaration
/// (`var api_key: String = "..."`) redact the assigned literal instead of
/// treating the type name as the value. A bare value runs to whitespace,
/// a quote, `,` or `;` — `&` is part of it (passwords contain `&`) except
/// in a query string, which `redact_key_value` trims.
static KEY_VALUE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?i)(?P<key>[A-Za-z0-9_.\-]*(?:api[_\-]?key|apikey|secret|secret[_\-]?key|token|passwd|password|passphrase|pwd|access[_\-]?key|private[_\-]?key|service[_\-]?role[_\-]?key|anon[_\-]?key|credentials?))(?P<sep>["']?\s*(?::\s*[A-Za-z_][A-Za-z0-9_]*\s*)?[:=]\s*)(?:"(?P<dq>[^"\n]*)"|'(?P<sq>[^'\n]*)'|(?P<bare>[^\s"'`,;]+))"#,
    )
    .expect("key/value pattern must compile")
});

/// Returns `text` with every detected credential replaced by `[redacted]`.
pub fn redact(text: &str) -> String {
    let mut out = text.to_string();

    for re in TOKEN_PATTERNS.iter() {
        if re.is_match(&out) {
            out = re.replace_all(&out, REDACTED).into_owned();
        }
    }

    out = AUTH_HEADER
        .replace_all(&out, |c: &Captures| {
            let val = &c["val"];
            if val == REDACTED {
                c[0].to_string()
            } else {
                format!("{}{REDACTED}", &c["pre"])
            }
        })
        .into_owned();

    out = BEARER
        .replace_all(&out, |c: &Captures| format!("{}{REDACTED}", &c["pre"]))
        .into_owned();

    out = URL_USERINFO
        .replace_all(&out, |c: &Captures| {
            format!("{}{REDACTED}{}", &c["pre"], &c["post"])
        })
        .into_owned();

    out = redact_key_values(&out);

    out
}

/// Key words weak enough on their own that plain lowercase prose after them
/// ("Secret: underground passage") is game text, not a credential.
fn is_common_word_key(key: &str) -> bool {
    key.eq_ignore_ascii_case("secret") || key.eq_ignore_ascii_case("token")
}

/// Keys whose value is a human-chosen password: short and wordlike values
/// are still secrets.
fn is_password_key(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    ["password", "passwd", "passphrase", "pwd", "secret"]
        .iter()
        .any(|w| k.ends_with(w))
}

/// Runs `KEY_VALUE` over `text` by hand rather than with `replace_all`,
/// because a query-string match must stop consuming at `&` so the next
/// parameter (`&token=...`) is still scanned.
fn redact_key_values(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    while let Some(c) = KEY_VALUE.captures_at(text, last) {
        let start = c.get(0).map_or(last, |m| m.start());
        let (replacement, end) = redact_key_value(text, &c);
        out.push_str(&text[last..start]);
        out.push_str(&replacement);
        last = end;
    }
    out.push_str(&text[last..]);
    out
}

/// Returns the replacement for one match and the byte offset in `haystack`
/// where it ends.
fn redact_key_value(haystack: &str, c: &Captures) -> (String, usize) {
    let m = c.get(0).expect("group 0 always matches");
    let whole = m.as_str();
    let key = &c["key"];
    let prefix = format!("{key}{}", &c["sep"]);

    for (name, quote) in [("dq", '"'), ("sq", '\'')] {
        if let Some(v) = c.name(name) {
            let v = v.as_str();
            let prose = is_common_word_key(key) && v.trim().contains(' ');
            let replacement = if should_redact_quoted(v) && !prose {
                format!("{prefix}{quote}{REDACTED}{quote}")
            } else {
                whole.to_string()
            };
            return (replacement, m.end());
        }
    }

    let Some(bare) = c.name("bare") else {
        return (whole.to_string(), m.end());
    };
    // In a query string (`?token=abc&x=1`) the value ends at `&`; the rest
    // is left for the next scan.
    let in_query = haystack[..m.start()].ends_with(['?', '&']);
    let value = match bare.as_str().find('&') {
        Some(i) if in_query => &bare.as_str()[..i],
        _ => bare.as_str(),
    };
    let end = bare.start() + value.len();
    let replacement = if should_redact_bare(key, &c["sep"], value) {
        format!("{prefix}{REDACTED}")
    } else {
        haystack[m.start()..end].to_string()
    };
    (replacement, end)
}

/// A quoted literal assigned to a credential-named key is treated as a
/// secret unless it's empty or already redacted.
fn should_redact_quoted(value: &str) -> bool {
    let v = value.trim();
    !v.is_empty() && !v.contains(REDACTED)
}

/// GDScript/common type names: `var password: String` declares a type, it
/// doesn't assign a value.
const TYPE_NAMES: &[&str] = &[
    "string",
    "stringname",
    "int",
    "float",
    "bool",
    "variant",
    "array",
    "dictionary",
    "packedbytearray",
    "packedstringarray",
    "object",
    "node",
    "str",
    "number",
    "boolean",
    "any",
];

/// An unquoted value is only redacted when it could plausibly be a secret:
/// not a number, boolean, call, node path, collection or type, and long
/// enough — 4 characters for password-like keys, 8 otherwise.
fn should_redact_bare(key: &str, sep: &str, value: &str) -> bool {
    let v = value.trim_end_matches(['.', ')', ']', '}', ':']);
    if v.contains(REDACTED) || v.parse::<f64>().is_ok() {
        return false;
    }
    let lower = v.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        "true" | "false" | "null" | "none" | "nil" | "undefined" | "required" | "optional"
    ) {
        return false;
    }
    // `name: Type` with no assignment is a declaration.
    if sep.trim_start_matches(['"', '\'']).trim() == ":" && TYPE_NAMES.contains(&lower.as_str()) {
        return false;
    }
    // Code rather than a literal: node paths (`$Node`, `%Unique`), calls
    // (`get_token()`), collections.
    if v.starts_with(['$', '%', '[', '{', '<']) || looks_like_call(v) {
        return false;
    }
    // "Secret: underground passage" / "The token: collectible coins".
    if is_common_word_key(key) && v.chars().all(|ch| ch.is_ascii_lowercase()) {
        return false;
    }
    let min_len = if is_password_key(key) { 4 } else { 8 };
    v.chars().count() >= min_len
}

/// `ident(` or `a.b.c(` at the start: a function/method call.
fn looks_like_call(v: &str) -> bool {
    match v.find('(') {
        Some(i) if i > 0 => v[..i]
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '.'),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// (input, expected output) pairs that must redact.
    const REDACTS: &[(&str, &str)] = &[
        (
            "my key is sk-ant-api03-AbCdEfGhIjKlMnOpQrStUvWxYz0123456789 ok",
            "my key is [redacted] ok",
        ),
        (
            "OPENAI: sk-proj-abcdefghijklmnopqrstuvwxyz012345",
            "OPENAI: [redacted]",
        ),
        ("sk-abcdefghijklmnopqrstuvwxyz0123456789ABCD", "[redacted]"),
        ("ghp_abcdefghijklmnopqrstuvwxyz0123456789", "[redacted]"),
        (
            "github_pat_11ABCDEFG0123456789_abcdefghijklmnopqrstuv",
            "[redacted]",
        ),
        ("aws id AKIAIOSFODNN7EXAMPLE here", "aws id [redacted] here"),
        ("stripe sk_live_51HabcdEFGhijkLMNop", "stripe [redacted]"),
        ("rk_test_abcdefghij1234", "[redacted]"),
        ("whsec_abcdefghijklmnop1234", "[redacted]"),
        (
            "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJyb2xlIjoic2VydmljZV9yb2xlIn0.dBjftJeZ4CVPmB92K27uhbUJU1p1r_wW1gFWFOEjXk",
            "[redacted]",
        ),
        ("sb_secret_abcdefghijklmnop", "[redacted]"),
        ("AIzaSyA1234567890abcdefghijklmnopqrstu", "[redacted]"),
        (
            "Authorization: Bearer abc123.def456-ghi",
            "Authorization: Bearer [redacted]",
        ),
        (
            r#"-H "authorization: token s3cr3tvalue""#,
            r#"-H "authorization: token [redacted]""#,
        ),
        (
            "use bearer abcdefghijklmnop123 for that",
            "use bearer [redacted] for that",
        ),
        ("api_key=abcd1234efgh", "api_key=[redacted]"),
        (
            "ANTHROPIC_API_KEY=not-a-known-format-but-secret",
            "ANTHROPIC_API_KEY=[redacted]",
        ),
        (r#"{"api_key": "hunter2"}"#, r#"{"api_key": "[redacted]"}"#),
        ("token: abcdefgh12345", "token: [redacted]"),
        ("password = 'hunter2'", "password = '[redacted]'"),
        ("Password: swordfish", "Password: [redacted]"),
        ("client_secret=xyzxyzxyz", "client_secret=[redacted]"),
        (
            r#"var api_key: String = "abc""#,
            r#"var api_key: String = "[redacted]""#,
        ),
        (
            "SUPABASE_SERVICE_ROLE_KEY=whatever-value-it-is",
            "SUPABASE_SERVICE_ROLE_KEY=[redacted]",
        ),
        (
            "https://api.example.com/v1?access_token=abc123def456&x=1",
            "https://api.example.com/v1?access_token=[redacted]&x=1",
        ),
        (
            "postgres://admin:s3cret@db.example.com/game",
            "postgres://admin:[redacted]@db.example.com/game",
        ),
        (
            "-----BEGIN RSA PRIVATE KEY-----\nMIIEow\nIBAAK\n-----END RSA PRIVATE KEY-----\nafter",
            "[redacted]\nafter",
        ),
        (
            "-----BEGIN PGP PRIVATE KEY BLOCK-----\n\nlQOYBF0\n=abcd\n-----END PGP PRIVATE KEY BLOCK-----",
            "[redacted]",
        ),
        (
            "-----BEGIN PGP PRIVATE KEY BLOCK-----\nlQOYBF0 (cut off",
            "[redacted]",
        ),
        // Short, symbol-bearing, `&`-containing passwords (review repros).
        ("password=hunter2", "password=[redacted]"),
        ("password: p&ssw0rd!", "password: [redacted]"),
        ("DB_PASSWORD=hunt$r2%x", "DB_PASSWORD=[redacted]"),
        ("pwd=Tr0ub4dor&3", "pwd=[redacted]"),
        (
            "https://x.example/login?user=ada&password=abcd&next=home&token=abcdefgh1234",
            "https://x.example/login?user=ada&password=[redacted]&next=home&token=[redacted]",
        ),
        // Credential-named variables still redact quoted prose.
        (
            r#"api_key = "some words here""#,
            r#"api_key = "[redacted]""#,
        ),
        ("secret: Xk9vQ2mR", "secret: [redacted]"),
        (
            "-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXktdjEAAAA (truncated paste",
            "[redacted]",
        ),
    ];

    /// Normal game text and GDScript that must come through untouched.
    const KEEPS: &[&str] = &[
        "var speed = 10",
        "func _ready():",
        "The player collects a token to open the gate.",
        "Each token: worth 5 coins.",
        "Collect the token: it opens the gate",
        "var token_count = 3",
        "var secret_door := $SecretDoor",
        "@onready var token = $Token",
        "var token = get_token()",
        "var password: String",
        "var token: Node2D = null",
        "const MAX_KEYS = 5",
        "@export var jump_height: float = 120.0",
        "var password_hint = \"it is your dog's name\"",
        "The knight is the bearer of bad news.",
        "task-list-for-the-first-sprint and risk-assessment-notes",
        "extends CharacterBody2D\n\nconst SPEED = 300.0\nconst JUMP_VELOCITY = -400.0\n\nfunc _physics_process(delta):\n\tif not is_on_floor():\n\t\tvelocity += get_gravity() * delta\n",
        "Commit 3f2a9c1e8b7d6a5f4e3d2c1b0a9f8e7d6c5b4a3f added the boss fight.",
        "Thread 20260925T142233123-a1b2c3d4 session 7f9c2ba4-e88f-11ec-8ea0-0242ac120002",
        "input.is_action_pressed(\"ui_accept\")",
        "password = \"\"",
        "token: true",
        "api_key: 123456789",
        "Visit https://godotengine.org/docs for help",
        "res://scenes/sk-level-boss-fight-arena.tscn",
        "Secret: underground passage",
        "The token: collectible coins",
        r#"var secret = "the old mill behind the waterfall""#,
        "var password: String",
        "var password = get_password()",
        "@onready var pwd = %PasswordField",
    ];

    #[test]
    fn redacts_known_credential_formats() {
        for (input, expected) in REDACTS {
            assert_eq!(&redact(input), expected, "input: {input:?}");
        }
    }

    #[test]
    fn leaves_normal_game_text_and_gdscript_alone() {
        for input in KEEPS {
            assert_eq!(&redact(input), input, "false positive on: {input:?}");
        }
    }

    #[test]
    fn redaction_is_idempotent() {
        for (input, _) in REDACTS {
            let once = redact(input);
            assert_eq!(redact(&once), once, "input: {input:?}");
        }
    }
}
