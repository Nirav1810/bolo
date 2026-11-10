//! Layer 1 formatting: the fixed pipeline of conservative rule passes that runs on the
//! instant path (ADR-0015).
//!
//! The passes run in a fixed order:
//!
//! ```text
//! fillers → self-corrections → commands and lists → numbers → punctuation fix-ups → render
//! ```
//!
//! Every pass follows the same rule: when unsure, leave the text verbatim. Polish can catch a miss, but nothing restores a
//! lost word.
//!
//! The pipeline is idempotent, never returns an empty string for non-empty input, and
//! stays well under 1 ms on a dictation (the latency gate in `tests/latency.rs`).

mod commands;
mod corrections;
mod fillers;
mod kinds;
mod lists;
mod numbers;
mod punctuation;
mod token;

/// The language of the dictation being formatted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Language {
    #[default]
    English,
    /// Hindi-English mixed speech in Roman (or Devanagari) script.
    Hinglish,
}

impl Language {
    /// The language of engine output when the engine's own multilingual mode decided
    /// (Deepgram `multi`). Hindi written in Devanagari shows Hinglish; anything else
    /// counts as English, so the Hinglish-only rules stay off when unsure.
    pub fn of_engine_output(raw: &str) -> Self {
        match crate::language::detect(raw) {
            crate::language::Detected::Hinglish => Self::Hinglish,
            _ => Self::English,
        }
    }
}

/// The markup the target app renders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Markup {
    #[default]
    Plain,
    Markdown,
}

/// Where the text is going, as far as formatting cares. `bolo-core` maps its
/// `TargetContext` onto this; `bolo-format` does not depend on `bolo-core`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FormatContext {
    pub markup: Markup,
    /// Code editors and terminals: no automatic capitals and no final period.
    pub code_mode: bool,
}

impl FormatContext {
    pub fn plain() -> Self {
        Self::default()
    }

    pub fn markdown() -> Self {
        Self { markup: Markup::Markdown, code_mode: false }
    }

    pub fn code() -> Self {
        Self { markup: Markup::Plain, code_mode: true }
    }
}

/// A dictation's text in both renderings, from one pipeline run.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Formatted {
    pub plain: String,
    pub markdown: String,
}

/// Run the Layer 1 pipeline on a dictation's text and render it for the target's markup.
pub fn apply(text: &str, language: Language, ctx: FormatContext) -> String {
    finish(token::render(&run(text, language, ctx.code_mode), ctx.markup), text)
}

/// Run the Layer 1 pipeline once and render the result as plain text and as Markdown.
/// They differ only around lists: Markdown leaves a blank line before and after one.
pub fn format(text: &str, language: Language, code_mode: bool) -> Formatted {
    let tokens = run(text, language, code_mode);
    Formatted {
        plain: finish(token::render(&tokens, Markup::Plain), text),
        markdown: finish(token::render(&tokens, Markup::Markdown), text),
    }
}

fn run(text: &str, language: Language, code_mode: bool) -> Vec<token::Token> {
    let mut tokens = token::tokenize(text);
    fillers::apply(&mut tokens, language);
    corrections::apply(&mut tokens, language);
    commands::apply(&mut tokens);
    lists::apply(&mut tokens);
    numbers::apply(&mut tokens);
    punctuation::apply(&mut tokens, code_mode);
    tokens
}

/// Never hand back an empty string: when nothing is left (only punctuation or spaces),
/// return the input, trimmed unless it is all whitespace.
fn finish(out: String, text: &str) -> String {
    if out.is_empty() {
        let trimmed = text.trim();
        return if trimmed.is_empty() { text } else { trimmed }.to_owned();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_gets_capital_and_period() {
        assert_eq!(apply("hello world", Language::English, FormatContext::plain()), "Hello world.");
    }

    #[test]
    fn punctuation_only_input_is_kept() {
        assert_eq!(apply(" , ", Language::English, FormatContext::plain()), ",");
    }
}
