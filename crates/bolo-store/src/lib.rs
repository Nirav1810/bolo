//! Settings (versioned TOML), history (SQLite) and secrets abstraction (M3).
//!
//! Secrets: Windows Credential Manager via `keyring`; Android encrypts with the Keystore
//! on the Kotlin side and hands keys to Rust per call. Keys are never written to plain
//! files or logs.
