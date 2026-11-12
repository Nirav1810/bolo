//! Formatting layers (M2):
//! 1. rules engine — spoken commands, filler removal, ordinal lists, dictionary, snippets
//! 2. optional LLM cleanup (Claude Haiku BYOK / local Qwen3 on desktop)
//! 3. output formatter — Plain / Markdown / HTML per target app
//!
//! Available now: [`translit`] (Devanagari → Roman Hinglish), needed because Nemotron
//! and Qwen3-ASR emit Hindi in Devanagari; [`urdu`] (Urdu script → Devanagari, for the
//! Hindi Deepgram `multi` writes in Urdu script); [`brahmic`] (the other Indic scripts →
//! Devanagari, for the same reason); [`language`], which tells English from
//! Hinglish for the Auto language mode; [`rules`], the Layer 1 pipeline (ADR-0015); and
//! [`snippets`], spoken triggers expanded to saved text after Layer 1 and polish.

pub mod brahmic;
pub mod language;
#[cfg(feature = "llm")]
pub mod llm;
pub mod loanwords;
pub mod rules;
pub mod snippets;
pub mod translit;
pub mod urdu;
