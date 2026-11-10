//! Layer 1 formatting: the fixed pipeline of conservative rule passes that runs on the
//! instant path (ADR-0015).
//!
//! The passes run in a fixed order:
//!
//! ```text
//! fillers → self-corrections → commands and lists → numbers → punctuation fix-ups → render
//! ```
//!
//! One run renders plain text, Markdown and HTML ([`format`]).
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
    /// `<p>` paragraphs, `<br>` line breaks and `<ol><li>` lists, every text node escaped.
    Html,
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

    pub fn html() -> Self {
        Self { markup: Markup::Html, code_mode: false }
    }
}

/// A dictation's text in every rendering, from one pipeline run.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Formatted {
    pub plain: String,
    pub markdown: String,
    pub html: String,
}

/// Run the Layer 1 pipeline on a dictation's text and render it for the target's markup.
pub fn apply(text: &str, language: Language, ctx: FormatContext) -> String {
    let out = token::render(&run(text, language, ctx.code_mode), ctx.markup);
    if ctx.markup == Markup::Html { finish_html(out, text) } else { finish(out, text) }
}

/// Run the Layer 1 pipeline once and render the result as plain text, Markdown and HTML.
/// Plain and Markdown differ only around lists: Markdown leaves a blank line before and
/// after one.
pub fn format(text: &str, language: Language, code_mode: bool) -> Formatted {
    let tokens = run(text, language, code_mode);
    Formatted {
        plain: finish(token::render(&tokens, Markup::Plain), text),
        markdown: finish(token::render(&tokens, Markup::Markdown), text),
        html: finish_html(token::render(&tokens, Markup::Html), text),
    }
}

/// Render text that is already formatted (polish output, in its plain or Markdown
/// layout) as HTML, without running the passes: a blank line splits paragraphs, a single
/// newline is a `<br>`, and lines starting "1. ", "2. " … are an ordered list. The text
/// is never trusted as markup: every character of it is escaped.
pub fn to_html(text: &str) -> String {
    let mut tokens = token::tokenize(text);
    token::number_lines_to_items(&mut tokens);
    finish_html(token::render(&tokens, Markup::Html), text)
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

/// [`finish`] for HTML: when nothing is left, the input as one escaped paragraph.
fn finish_html(out: String, text: &str) -> String {
    if out.is_empty() && !text.is_empty() {
        let mut html = String::from("<p>");
        token::escape_html(&finish(out, text), &mut html);
        html.push_str("</p>");
        return html;
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

    #[test]
    fn format_renders_all_three_from_one_run() {
        let f = format("first milk second eggs third bread", Language::English, false);
        assert_eq!(f.plain, "1. Milk\n2. Eggs\n3. Bread");
        assert_eq!(f.markdown, "1. Milk\n2. Eggs\n3. Bread");
        assert_eq!(f.html, "<ol><li>Milk</li><li>Eggs</li><li>Bread</li></ol>");
    }

    #[test]
    fn html_of_nothing_left_is_the_escaped_input() {
        assert_eq!(apply("um", Language::English, FormatContext::html()), "<p>um</p>");
        assert_eq!(to_html(""), "");
    }
}
