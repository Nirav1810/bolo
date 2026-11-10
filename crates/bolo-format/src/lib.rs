//! Formatting layers (M2):
//! 1. rules engine — spoken commands, filler removal, ordinal lists, dictionary, snippets
//! 2. optional LLM cleanup (Claude Haiku BYOK / local Qwen3 on desktop)
//! 3. output formatter — Plain / Markdown / HTML per target app
//!
//! Available now: [`translit`] (Devanagari → Roman Hinglish), needed because Nemotron
//! and Qwen3-ASR emit Hindi in Devanagari; [`language`], which tells English from
//! Hinglish for the Auto language mode; and [`rules`], the Layer 1 pipeline (ADR-0015).

pub mod language;
#[cfg(feature = "llm")]
pub mod llm;
pub mod loanwords;
pub mod rules;
pub mod translit;
