//! API keys in the OS credential store: Windows Credential Manager, service `bolo`, one
//! entry per provider (the provider name is the entry's user name).
//!
//! Keys are loaded into [`ApiKeys`] for the engine. They never reach the settings file,
//! a log line or a `Debug` string; errors carry only the store's own message.

use std::sync::Arc;

use bolo_core::{ANTHROPIC, ApiKeys};
use keyring_core::{CredentialStore, Error as KeyringError};

use crate::{Result, StoreError};

/// The credential service name.
pub const SERVICE: &str = "bolo";

/// The providers Bolo stores keys for. The `-admin` ones are optional usage keys, which
/// only read the provider's usage reports (the desktop app's `billing`).
pub const PROVIDERS: [&str; 6] = ["deepgram", "groq", "sarvam", ANTHROPIC, "deepgram-admin", "anthropic-admin"];

pub struct Secrets {
    store: Arc<CredentialStore>,
    keys: ApiKeys,
}

impl std::fmt::Debug for Secrets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Secrets").field("store", &self.store.vendor()).field("keys", &self.keys).finish()
    }
}

impl Secrets {
    /// The OS store: Windows Credential Manager.
    #[cfg(windows)]
    pub fn os() -> Result<Self> {
        Self::with_store(windows_native_keyring_store::Store::new()?)
    }

    /// Keys from `store` (tests pass keyring's mock store). A provider with no entry has
    /// no key; one that can't be read is logged by name and skipped.
    pub fn with_store(store: Arc<CredentialStore>) -> Result<Self> {
        let mut keys = ApiKeys::default();
        for provider in PROVIDERS {
            match store.build(SERVICE, provider, None)?.get_password() {
                Ok(key) => keys.set(provider, key),
                Err(KeyringError::NoEntry) => {}
                Err(e) => tracing::warn!(provider, error = %e, "can't read the API key; skipping it"),
            }
        }
        Ok(Self { store, keys })
    }

    /// The keys for [`EngineConfig::api_keys`](bolo_core::EngineConfig::api_keys).
    pub fn api_keys(&self) -> &ApiKeys {
        &self.keys
    }

    pub fn has(&self, provider: &str) -> bool {
        self.keys.get(provider).is_some()
    }

    /// What the settings window shows for a stored key: its last four characters (all of
    /// a key that short is hidden). `None` when there's no key.
    pub fn hint(&self, provider: &str) -> Option<String> {
        let key = self.keys.get(provider)?;
        let chars: Vec<char> = key.chars().collect();
        Some(if chars.len() <= 8 { String::new() } else { chars[chars.len() - 4..].iter().collect() })
    }

    /// Stores `key` for `provider`, trimmed. A blank key deletes the entry.
    pub fn set(&mut self, provider: &str, key: &str) -> Result<()> {
        if !PROVIDERS.contains(&provider) {
            return Err(StoreError::UnknownProvider(provider.into()));
        }
        let entry = self.store.build(SERVICE, provider, None)?;
        let key = key.trim();
        if key.is_empty() {
            match entry.delete_credential() {
                Ok(()) | Err(KeyringError::NoEntry) => {}
                Err(e) => return Err(e.into()),
            }
        } else {
            entry.set_password(key)?;
        }
        self.keys.set(provider, key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use keyring_core::mock;

    use super::*;

    fn store() -> Arc<CredentialStore> {
        mock::Store::new().unwrap()
    }

    #[test]
    fn keys_round_trip_through_the_store() {
        let store = store();
        let mut s = Secrets::with_store(store.clone()).unwrap();
        assert!(!s.has(ANTHROPIC));
        s.set(ANTHROPIC, "  sk-ant-123 \n").unwrap();
        s.set("deepgram", "dg-456").unwrap();
        assert_eq!(s.api_keys().get(ANTHROPIC), Some("sk-ant-123"));

        // A fresh load reads what was stored, under service `bolo`, user = provider.
        let again = Secrets::with_store(store.clone()).unwrap();
        assert_eq!(again.api_keys().get(ANTHROPIC), Some("sk-ant-123"));
        assert_eq!(again.api_keys().get("deepgram"), Some("dg-456"));
        assert_eq!(again.api_keys().get("groq"), None);
        assert_eq!(store.build(SERVICE, "deepgram", None).unwrap().get_password().unwrap(), "dg-456");
    }

    #[test]
    fn the_hint_is_the_last_four_characters_of_a_long_key() {
        let mut s = Secrets::with_store(store()).unwrap();
        assert_eq!(s.hint("groq"), None);
        s.set("groq", "gsk_abcdefgh1234").unwrap();
        assert_eq!(s.hint("groq").as_deref(), Some("1234"));
        s.set("sarvam", "short").unwrap();
        assert_eq!(s.hint("sarvam").as_deref(), Some(""));
    }

    #[test]
    fn a_blank_key_deletes_the_entry() {
        let store = store();
        let mut s = Secrets::with_store(store.clone()).unwrap();
        s.set("groq", "gsk").unwrap();
        s.set("groq", "   ").unwrap();
        s.set("sarvam", "").unwrap(); // never stored: still fine
        assert!(!s.has("groq"));
        assert!(!Secrets::with_store(store).unwrap().has("groq"));
    }

    #[test]
    fn unknown_providers_are_refused() {
        let mut s = Secrets::with_store(store()).unwrap();
        assert!(matches!(s.set("openai", "k"), Err(StoreError::UnknownProvider(p)) if p == "openai"));
    }

    #[test]
    fn an_unreadable_key_is_skipped_not_fatal() {
        let store = store();
        let mut s = Secrets::with_store(store.clone()).unwrap();
        s.set("groq", "gsk").unwrap();
        s.set(ANTHROPIC, "sk-ant").unwrap();
        let entry = store.build(SERVICE, "groq", None).unwrap();
        let cred: &mock::Cred = entry.as_any().downcast_ref().unwrap();
        cred.set_error(KeyringError::BadEncoding(b"gsk".to_vec()));
        let again = Secrets::with_store(store).unwrap();
        assert!(!again.has("groq"));
        assert!(again.has(ANTHROPIC));
    }

    /// The real Credential Manager: opens and reads (never writes the `bolo` keys), and
    /// round-trips a throwaway entry under another service. Run by hand on Windows.
    #[cfg(windows)]
    #[test]
    #[ignore = "touches the user's Credential Manager"]
    fn windows_credential_manager_works() {
        let s = Secrets::os().unwrap();
        println!("{s:?}");
        let store: Arc<CredentialStore> = windows_native_keyring_store::Store::new().unwrap();
        let entry = store.build("bolo-store-test", "probe", None).unwrap();
        entry.set_password("probe-value").unwrap();
        assert_eq!(entry.get_password().unwrap(), "probe-value");
        entry.delete_credential().unwrap();
        assert!(matches!(entry.get_password(), Err(KeyringError::NoEntry)));
    }

    #[test]
    fn keys_never_show_in_debug_or_errors() {
        let store = store();
        let mut s = Secrets::with_store(store.clone()).unwrap();
        s.set(ANTHROPIC, "sk-ant-SECRET").unwrap();
        let debug = format!("{s:?}");
        assert!(!debug.contains("SECRET") && debug.contains("anthropic"), "{debug}");

        let entry = store.build(SERVICE, "groq", None).unwrap();
        let cred: &mock::Cred = entry.as_any().downcast_ref().unwrap();
        cred.set_error(KeyringError::BadEncoding(b"gsk-SECRET".to_vec()));
        let err = s.set("groq", "gsk-SECRET").unwrap_err();
        assert!(!format!("{err} {err:?}").contains("SECRET"), "{err:?}");
    }
}
