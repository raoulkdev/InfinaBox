//! Provider keys in the operating system's keychain (spec §7.3, §8.1). A
//! value never leaves this module except into the request that uses it: the
//! app's commands only say whether a name is connected.
//!
//! `KeychainStore` is the real thing (macOS Keychain, Windows Credential
//! Manager, Linux Secret Service); `MemoryStore` is for tests and for
//! machines without a keychain service.
//!
//! Phase C contract (frozen). Wave 0 stub — task SK fills in the bodies.

use std::collections::HashMap;
use std::sync::Mutex;

use anyhow::{Result, bail};
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
}

pub trait SecretStore: Send + Sync {
    fn get(&self, name: SecretName) -> Result<Option<String>>;
    fn set(&self, name: SecretName, value: &str) -> Result<()>;
    fn delete(&self, name: SecretName) -> Result<()>;
    fn is_set(&self, name: SecretName) -> Result<bool> {
        Ok(self.get(name)?.is_some())
    }
}

/// The OS keychain, service name `infinabox`.
pub struct KeychainStore;

impl SecretStore for KeychainStore {
    fn get(&self, name: SecretName) -> Result<Option<String>> {
        let _ = name;
        bail!("not implemented yet (Phase C, task SK)")
    }
    fn set(&self, name: SecretName, value: &str) -> Result<()> {
        let _ = (name, value);
        bail!("not implemented yet (Phase C, task SK)")
    }
    fn delete(&self, name: SecretName) -> Result<()> {
        let _ = name;
        bail!("not implemented yet (Phase C, task SK)")
    }
}

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
