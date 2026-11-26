//! API keys on Android (ADR-0023): Kotlin keeps them, encrypted with an AES-GCM key in
//! the Android Keystore, and implements [`SecretStore`]. [`ForeignStore`] wraps that as a
//! `keyring_core` credential store, so `bolo_store::Secrets::with_store` works as on the
//! desktop.
//!
//! An entry's key is `<service>/<user>`, e.g. `bolo/deepgram`. Values never reach a log,
//! a `Debug` string or an error message; the Kotlin side must keep them out of its
//! exceptions too.

use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;

use keyring_core::api::{Credential, CredentialApi, CredentialStoreApi};
use keyring_core::{CredentialStore, Entry, Error as KeyringError};

/// A failed store call. `detail` names what failed, never a value.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum SecretError {
    #[error("{detail}")]
    Failed { detail: String },
}

impl From<uniffi::UnexpectedUniFFICallbackError> for SecretError {
    fn from(e: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::Failed { detail: e.reason }
    }
}

/// Encrypted key-value storage, implemented in Kotlin.
#[uniffi::export(with_foreign)]
pub trait SecretStore: Send + Sync {
    /// The value stored under `key`, or `None`.
    fn get(&self, key: String) -> Result<Option<String>, SecretError>;
    /// Stores `value` under `key`, replacing any value there.
    fn set(&self, key: String, value: String) -> Result<(), SecretError>;
    /// Removes `key`. Whether there was a value.
    fn delete(&self, key: String) -> Result<bool, SecretError>;
}

/// A [`SecretStore`] as a `keyring_core` credential store.
pub struct ForeignStore(Arc<dyn SecretStore>);

impl ForeignStore {
    /// `store` as a credential store for `bolo_store::Secrets::with_store`.
    pub fn wrap(store: Arc<dyn SecretStore>) -> Arc<CredentialStore> {
        Arc::new(Self(store))
    }
}

impl std::fmt::Debug for ForeignStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ForeignStore")
    }
}

impl CredentialStoreApi for ForeignStore {
    fn vendor(&self) -> String {
        "bolo-ffi SecretStore (Android Keystore)".into()
    }

    fn id(&self) -> String {
        "bolo-ffi".into()
    }

    fn build(&self, service: &str, user: &str, _: Option<&HashMap<&str, &str>>) -> keyring_core::Result<Entry> {
        let cred = Cred { store: self.0.clone(), service: service.into(), user: user.into() };
        Ok(Entry::new_with_credential(Arc::new(cred) as Arc<Credential>))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// One `<service>/<user>` entry.
struct Cred {
    store: Arc<dyn SecretStore>,
    service: String,
    user: String,
}

impl std::fmt::Debug for Cred {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cred").field("service", &self.service).field("user", &self.user).finish()
    }
}

impl Cred {
    fn key(&self) -> String {
        format!("{}/{}", self.service, self.user)
    }
}

fn failed(e: SecretError) -> KeyringError {
    KeyringError::PlatformFailure(Box::new(e))
}

impl CredentialApi for Cred {
    fn set_secret(&self, secret: &[u8]) -> keyring_core::Result<()> {
        let value = String::from_utf8(secret.to_vec())
            .map_err(|_| KeyringError::Invalid("secret".into(), "not UTF-8".into()))?;
        self.store.set(self.key(), value).map_err(failed)
    }

    fn get_secret(&self) -> keyring_core::Result<Vec<u8>> {
        match self.store.get(self.key()).map_err(failed)? {
            Some(value) => Ok(value.into_bytes()),
            None => Err(KeyringError::NoEntry),
        }
    }

    fn delete_credential(&self) -> keyring_core::Result<()> {
        if self.store.delete(self.key()).map_err(failed)? { Ok(()) } else { Err(KeyringError::NoEntry) }
    }

    fn get_credential(&self) -> keyring_core::Result<Option<Arc<Credential>>> {
        self.get_secret()?;
        Ok(None)
    }

    fn get_specifiers(&self) -> Option<(String, String)> {
        Some((self.service.clone(), self.user.clone()))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use bolo_core::ANTHROPIC;
    use bolo_store::{SERVICE, Secrets, StoreError};
    use keyring_core::mock;

    use super::*;

    /// A Kotlin-like store backed by keyring's mock store, so the adapter is tested
    /// against keyring's own reference behaviour. `fail` makes every call fail, quoting
    /// the value it was given the way a careless implementation might not.
    struct MockBacked {
        mock: Arc<CredentialStore>,
        fail: Mutex<bool>,
    }

    impl MockBacked {
        fn new() -> Arc<Self> {
            Arc::new(Self { mock: mock::Store::new().unwrap(), fail: Mutex::new(false) })
        }

        fn entry(&self, key: &str) -> Entry {
            let (service, user) = key.split_once('/').unwrap();
            self.mock.build(service, user, None).unwrap()
        }

        fn check(&self) -> Result<(), SecretError> {
            if *self.fail.lock().unwrap() {
                return Err(SecretError::Failed { detail: "keystore unavailable".into() });
            }
            Ok(())
        }
    }

    impl SecretStore for MockBacked {
        fn get(&self, key: String) -> Result<Option<String>, SecretError> {
            self.check()?;
            match self.entry(&key).get_password() {
                Ok(v) => Ok(Some(v)),
                Err(KeyringError::NoEntry) => Ok(None),
                Err(e) => Err(SecretError::Failed { detail: e.to_string() }),
            }
        }
        fn set(&self, key: String, value: String) -> Result<(), SecretError> {
            self.check()?;
            self.entry(&key).set_password(&value).map_err(|e| SecretError::Failed { detail: e.to_string() })
        }
        fn delete(&self, key: String) -> Result<bool, SecretError> {
            self.check()?;
            match self.entry(&key).delete_credential() {
                Ok(()) => Ok(true),
                Err(KeyringError::NoEntry) => Ok(false),
                Err(e) => Err(SecretError::Failed { detail: e.to_string() }),
            }
        }
    }

    #[test]
    fn secrets_round_trip_through_the_foreign_store() {
        let backing = MockBacked::new();
        let store = ForeignStore::wrap(backing.clone());
        let mut s = Secrets::with_store(store.clone()).unwrap();
        assert!(!s.has(ANTHROPIC));
        s.set(ANTHROPIC, " sk-ant-123 ").unwrap();
        s.set("deepgram", "dg-456").unwrap();

        // Stored under `bolo/<provider>` in the Kotlin store, trimmed.
        assert_eq!(backing.get(format!("{SERVICE}/{ANTHROPIC}")).unwrap().as_deref(), Some("sk-ant-123"));
        let again = Secrets::with_store(ForeignStore::wrap(backing.clone())).unwrap();
        assert_eq!(again.api_keys().get("deepgram"), Some("dg-456"));
        assert_eq!(again.api_keys().get("groq"), None);

        // A blank key deletes it; deleting what isn't there is fine.
        s.set("deepgram", "  ").unwrap();
        s.set("sarvam", "").unwrap();
        assert_eq!(backing.get(format!("{SERVICE}/deepgram")).unwrap(), None);
        assert!(!Secrets::with_store(store).unwrap().has("deepgram"));
    }

    #[test]
    fn store_failures_are_errors_without_the_value() {
        let backing = MockBacked::new();
        let mut s = Secrets::with_store(ForeignStore::wrap(backing.clone())).unwrap();
        s.set("groq", "gsk-kept").unwrap();
        *backing.fail.lock().unwrap() = true;
        let err = s.set("groq", "gsk-SECRET").unwrap_err();
        assert!(matches!(err, StoreError::Secrets(_)));
        let text = format!("{err} {err:?} {s:?}");
        assert!(!text.contains("SECRET") && !text.contains("gsk-kept"), "{text}");
        assert!(text.contains("keystore unavailable"), "{text}");

        // An unreadable key is skipped at load, not fatal.
        let loaded = Secrets::with_store(ForeignStore::wrap(backing)).unwrap();
        assert!(!loaded.has("groq"));
    }

    #[test]
    fn entries_say_what_they_are_and_never_show_values() {
        let backing = MockBacked::new();
        let store = ForeignStore::wrap(backing);
        let entry = store.build("bolo", "anthropic", None).unwrap();
        assert!(matches!(entry.get_password(), Err(KeyringError::NoEntry)));
        assert!(matches!(entry.delete_credential(), Err(KeyringError::NoEntry)));
        entry.set_password("sk-ant-SECRET").unwrap();
        assert_eq!(entry.get_password().unwrap(), "sk-ant-SECRET");
        assert_eq!(entry.get_specifiers(), Some(("bolo".into(), "anthropic".into())));
        let debug = format!("{entry:?} {store:?}");
        assert!(!debug.contains("SECRET"), "{debug}");
        entry.delete_credential().unwrap();
        assert!(matches!(entry.get_password(), Err(KeyringError::NoEntry)));
    }
}
