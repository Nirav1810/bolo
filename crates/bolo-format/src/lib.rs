//! Formatting layers (M2):
//! 1. rules engine — spoken commands, filler removal, ordinal lists, dictionary, snippets
//! 2. optional LLM cleanup (Claude Haiku BYOK / local Qwen3 on desktop)
//! 3. output formatter — Plain / Markdown / HTML per target app
//!
//! Available now: [`translit`] (Devanagari → Roman Hinglish), needed because Nemotron
//! and Qwen3-ASR emit Hindi in Devanagari.

#[cfg(feature = "llm")]
pub mod llm;
pub mod loanwords;
pub mod translit;
