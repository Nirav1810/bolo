//! Spoken layout commands: "new line" and "new paragraph", plus the Hinglish forms
//! ("nayi line", "naya paragraph"). Spoken punctuation ("comma", "full stop") is not a
//! command, because people say those words in normal speech (ADR-0015).
//!
//! "new line" is also an ordinary phrase ("a new line of products"), so a command is not
//! taken after a determiner or before "of".

use super::token::{Break, Token, push_break};

/// Words that, just before "new line", make it a noun phrase rather than a command.
const DETERMINERS: &[&str] = &[
    "a", "an", "the", "this", "that", "these", "those", "my", "your", "our", "their", "his", "her", "its", "another",
    "every", "each", "any", "some", "no", "ek", "koi", "yeh", "ye", "woh", "wo",
];

fn is_word(t: Option<&Token>, pred: impl Fn(&str) -> bool) -> bool {
    matches!(t, Some(Token::Word(w)) if pred(&w.to_lowercase()))
}

/// The command starting at `tokens[i]`, and how many word tokens it spans.
fn command_at(tokens: &[Token], i: usize) -> Option<(Break, usize)> {
    let Some(Token::Word(first)) = tokens.get(i) else { return None };
    let first = first.to_lowercase();
    if first == "newline" {
        return Some((Break::Line, 1));
    }
    let Some(Token::Word(second)) = tokens.get(i + 1) else { return None };
    let brk = match (first.as_str(), second.to_lowercase().as_str()) {
        ("new" | "nayi" | "nai" | "nayee" | "naya", "line") => Break::Line,
        ("new" | "naya" | "nayi" | "nai", "paragraph") => Break::Paragraph,
        _ => return None,
    };
    Some((brk, 2))
}

pub(super) fn apply(tokens: &mut Vec<Token>) {
    let src = std::mem::take(tokens);
    let out = tokens;
    out.reserve(src.len());
    let mut i = 0;
    while i < src.len() {
        if let Some((brk, len)) = command_at(&src, i)
            && !is_word(out.last(), |w| DETERMINERS.contains(&w))
            && !is_word(src.get(i + len), |w| w == "of")
        {
            // "Hello, new line, how are you" → "Hello\nhow are you": a comma or
            // semicolon before the break goes, and so does punctuation after the command.
            while matches!(out.last(), Some(Token::Punct(',' | ';'))) {
                out.pop();
            }
            push_break(out, brk);
            i += len;
            while matches!(src.get(i), Some(Token::Punct(_))) {
                i += 1;
            }
            continue;
        }
        match &src[i] {
            Token::Break(b) => push_break(out, *b),
            t => out.push(t.clone()),
        }
        i += 1;
    }
}
