//! Provider keys in the operating system's keychain (spec §7.3, §8.1). A
//! value never leaves this module except into the request that uses it: the
//! app's commands only say whether a name is connected.
//!
//! `KeychainStore` is the real thing (macOS Keychain, Windows Credential
//! Manager, Linux Secret Service); `MemoryStore` is for tests and for
//! machines without a keychain service.
//!
//! Phase C contract (frozen). Values are never put in an error, a `Debug`
//! string or a log line.

use std::collections::HashMap;
use std::sync::Mutex;

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};

/// Everything InfinaBox may keep for the person.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SecretName {
    CloudflareAccountId,
    CloudflareApiToken,
    FishAudioApiKey,
    ElevenLabsApiKey,
    AnthropicApiKey,
    OpenAiApiKey,
}

impl SecretName {
    /// The keychain entry name.
    pub fn key(self) -> &'static str {
        match self {
            SecretName::CloudflareAccountId => "cloudflare-account-id",
            SecretName::CloudflareApiToken => "cloudflare-api-token",
            SecretName::FishAudioApiKey => "fish-audio-api-key",
            SecretName::ElevenLabsApiKey => "elevenlabs-api-key",
            SecretName::AnthropicApiKey => "anthropic-api-key",
            SecretName::OpenAiApiKey => "openai-api-key",
        }
    }

    /// False only for the Cloudflare account id: it's an identifier, not a
    /// credential, but is kept with the rest. Every other name is only ever
    /// shown as set / not set.
    pub fn is_secret(self) -> bool {
        !matches!(self, SecretName::CloudflareAccountId)
    }
}

/// What the UI shows in place of a stored value: whether one is set, never
/// any character of it.
pub fn mask(value: &str) -> String {
    if value.is_empty() {
        String::new()
    } else {
        "••••".to_string()
    }
}

pub trait SecretStore: Send + Sync {
    fn get(&self, name: SecretName) -> Result<Option<String>>;
    fn set(&self, name: SecretName, value: &str) -> Result<()>;
    fn delete(&self, name: SecretName) -> Result<()>;
    fn is_set(&self, name: SecretName) -> Result<bool> {
        Ok(self.get(name)?.is_some())
    }
}

/// The OS keychain, service name `infinabox`, one entry per `SecretName::key()`.
pub struct KeychainStore;

const SERVICE: &str = "infinabox";

impl std::fmt::Debug for KeychainStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("KeychainStore")
    }
}

/// The one plain sentence every keychain failure becomes. `keyring`'s error
/// text describes the failure, never the value.
#[cfg(any(target_os = "macos", windows, target_os = "linux"))]
fn keychain_error(e: keyring::Error) -> anyhow::Error {
    anyhow!(
        "InfinaBox couldn't reach your computer's password storage (Keychain / Credential Manager / Secret Service). Details: {e}"
    )
}

#[cfg(any(target_os = "macos", windows, target_os = "linux"))]
fn entry(name: SecretName) -> Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, name.key()).map_err(keychain_error)
}

#[cfg(any(target_os = "macos", windows, target_os = "linux"))]
impl SecretStore for KeychainStore {
    fn get(&self, name: SecretName) -> Result<Option<String>> {
        match entry(name)?.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(keychain_error(e)),
        }
    }
    fn set(&self, name: SecretName, value: &str) -> Result<()> {
        entry(name)?.set_password(value).map_err(keychain_error)
    }
    fn delete(&self, name: SecretName) -> Result<()> {
        match entry(name)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(keychain_error(e)),
        }
    }
}

/// Platforms without a keychain backend: the same plain sentence.
#[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
impl SecretStore for KeychainStore {
    fn get(&self, _name: SecretName) -> Result<Option<String>> {
        Err(unsupported())
    }
    fn set(&self, _name: SecretName, _value: &str) -> Result<()> {
        Err(unsupported())
    }
    fn delete(&self, _name: SecretName) -> Result<()> {
        Err(unsupported())
    }
}

#[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
fn unsupported() -> anyhow::Error {
    let _ = SERVICE;
    anyhow!(
        "InfinaBox couldn't reach your computer's password storage (Keychain / Credential Manager / Secret Service). Details: this platform has no supported password storage"
    )
}

/// In-memory store for tests and machines without a keychain service.
#[derive(Default)]
pub struct MemoryStore(pub Mutex<HashMap<SecretName, String>>);

impl SecretStore for MemoryStore {
    fn get(&self, name: SecretName) -> Result<Option<String>> {
        Ok(self.0.lock().unwrap_or_else(|e| e.into_inner()).get(&name).cloned())
    }
    fn set(&self, name: SecretName, value: &str) -> Result<()> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).insert(name, value.to_string());
        Ok(())
    }
    fn delete(&self, name: SecretName) -> Result<()> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).remove(&name);
        Ok(())
    }
}

/// Shows which names are set, never their values.
impl std::fmt::Debug for MemoryStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let map = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let mut names: Vec<&str> = map.keys().map(|n| n.key()).collect();
        names.sort_unstable();
        f.debug_struct("MemoryStore").field("set", &names).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [SecretName; 6] = [
        SecretName::CloudflareAccountId,
        SecretName::CloudflareApiToken,
        SecretName::FishAudioApiKey,
        SecretName::ElevenLabsApiKey,
        SecretName::AnthropicApiKey,
        SecretName::OpenAiApiKey,
    ];

    /// The serde names are mirrored in TypeScript; changing one breaks the UI.
    #[test]
    fn serde_names_are_pinned() {
        let expected = [
            "cloudflare_account_id",
            "cloudflare_api_token",
            "fish_audio_api_key",
            "eleven_labs_api_key",
            "anthropic_api_key",
            "open_ai_api_key",
        ];
        for (name, want) in ALL.iter().zip(expected) {
            assert_eq!(serde_json::to_string(name).unwrap(), format!("\"{want}\""));
            let back: SecretName = serde_json::from_str(&format!("\"{want}\"")).unwrap();
            assert_eq!(back, *name);
        }
    }

    #[test]
    fn keychain_keys_are_distinct() {
        let mut keys: Vec<_> = ALL.iter().map(|n| n.key()).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), ALL.len());
    }

    #[test]
    fn only_the_account_id_is_not_secret() {
        for n in ALL {
            assert_eq!(n.is_secret(), n != SecretName::CloudflareAccountId, "{n:?}");
        }
    }

    #[test]
    fn mask_shows_only_that_a_value_is_set() {
        assert_eq!(mask(""), "");
        let m = mask("test-key-123");
        assert_eq!(m, "••••");
        assert!(!m.contains("test") && !m.contains('1'));
        assert_eq!(mask("x"), mask("a much longer test value"));
    }

    #[test]
    fn memory_store_round_trip() {
        let s = MemoryStore::default();
        let n = SecretName::AnthropicApiKey;
        assert_eq!(s.get(n).unwrap(), None);
        assert!(!s.is_set(n).unwrap());
        s.set(n, "test-key-123").unwrap();
        assert_eq!(s.get(n).unwrap().as_deref(), Some("test-key-123"));
        assert!(s.is_set(n).unwrap());
        s.set(n, "test-key-456").unwrap();
        assert_eq!(s.get(n).unwrap().as_deref(), Some("test-key-456"));
        s.delete(n).unwrap();
        assert_eq!(s.get(n).unwrap(), None);
        // Deleting what isn't there is fine.
        s.delete(n).unwrap();
    }

    #[test]
    fn memory_store_names_are_independent() {
        let s = MemoryStore::default();
        s.set(SecretName::OpenAiApiKey, "test-a").unwrap();
        assert_eq!(s.get(SecretName::AnthropicApiKey).unwrap(), None);
    }

    #[test]
    fn debug_output_never_contains_values() {
        let s = MemoryStore::default();
        s.set(SecretName::FishAudioApiKey, "test-key-123").unwrap();
        let shown = format!("{s:?}");
        assert!(!shown.contains("test-key-123"), "{shown}");
        assert!(shown.contains("fish-audio-api-key"));
        assert_eq!(format!("{KeychainStore:?}"), "KeychainStore");
    }

    /// Compile-only: `KeychainStore` is usable as a `SecretStore` trait object.
    #[allow(dead_code)]
    fn keychain_store_is_a_secret_store() -> Box<dyn SecretStore> {
        Box::new(KeychainStore)
    }

    #[test]
    #[ignore = "needs a system keychain; run with --ignored"]
    fn keychain_round_trip() {
        let store = KeychainStore;
        let name = SecretName::OpenAiApiKey;
        // Never run over a real saved value.
        assert!(
            store.get(name).unwrap().is_none(),
            "a real value is stored under {}; not touching it",
            name.key()
        );
        store.set(name, "test-key-123").unwrap();
        assert_eq!(store.get(name).unwrap().as_deref(), Some("test-key-123"));
        store.delete(name).unwrap();
        assert_eq!(store.get(name).unwrap(), None);
        store.delete(name).unwrap();
    }
}
