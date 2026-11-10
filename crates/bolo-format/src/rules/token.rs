//! Tokens the passes work on, and rendering them back to text.
//!
//! Text is split on whitespace. Sentence punctuation at the edges of a chunk becomes its
//! own token, so spacing is rebuilt from scratch at render time. Punctuation inside a
//! chunk stays put ("5:30", "3.5", "example.com", "1,000"). Newlines in the input become
//! line or paragraph breaks, so rendered output tokenizes back to the same tokens.

use super::FormatContext;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Token {
    Word(String),
    Punct(char),
    Break(Break),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Break {
    Line,
    Paragraph,
}

impl Break {
    fn as_str(self) -> &'static str {
        match self {
            Self::Line => "\n",
            Self::Paragraph => "\n\n",
        }
    }
}

/// Punctuation split off the end of a chunk.
fn is_trailing_punct(c: char) -> bool {
    matches!(c, ',' | '.' | ';' | ':' | '!' | '?' | '।' | '…')
}

/// Punctuation split off the start of a chunk (",hello"), when a word follows it. A
/// leading '.' or ':' is left alone (".net", ":)").
fn is_leading_punct(c: char) -> bool {
    matches!(c, ',' | ';' | '!' | '?' | '।')
}

pub(super) fn tokenize(text: &str) -> Vec<Token> {
    let mut tokens = Vec::with_capacity(text.len() / 4 + 1);
    let mut rest = text;
    while !rest.is_empty() {
        let ws_end = rest.find(|c: char| !c.is_whitespace()).unwrap_or(rest.len());
        let newlines = rest[..ws_end].matches('\n').count();
        if newlines > 0 {
            let brk = if newlines >= 2 { Break::Paragraph } else { Break::Line };
            push_break(&mut tokens, brk);
        }
        rest = &rest[ws_end..];
        let chunk_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        if chunk_end > 0 {
            push_chunk(&mut tokens, &rest[..chunk_end]);
        }
        rest = &rest[chunk_end..];
    }
    tokens
}

/// Add a break, merging it into a break just before it (the stronger one wins).
pub(super) fn push_break(tokens: &mut Vec<Token>, brk: Break) {
    if let Some(Token::Break(prev)) = tokens.last_mut() {
        *prev = (*prev).max(brk);
    } else {
        tokens.push(Token::Break(brk));
    }
}

fn push_chunk(tokens: &mut Vec<Token>, chunk: &str) {
    let lead_end = chunk.find(|c: char| !is_leading_punct(c)).unwrap_or(chunk.len());
    let (lead, body) = if chunk[lead_end..].starts_with(char::is_alphanumeric) {
        (&chunk[..lead_end], &chunk[lead_end..])
    } else {
        ("", chunk)
    };
    tokens.extend(lead.chars().map(Token::Punct));
    let word = body.trim_end_matches(is_trailing_punct);
    if !word.is_empty() {
        tokens.push(Token::Word(word.to_owned()));
    }
    tokens.extend(body[word.len()..].chars().map(Token::Punct));
}

/// Join tokens back into text: one space between words, punctuation attached to the word
/// before it, breaks as newlines.
pub(super) fn render(tokens: &[Token], ctx: FormatContext) -> String {
    let _ = ctx; // Markdown renders lists differently (ticket 06)
    let mut out = String::new();
    let mut after_break = true;
    for t in tokens {
        match t {
            Token::Word(w) => {
                if !after_break {
                    out.push(' ');
                }
                out.push_str(w);
                after_break = false;
            }
            Token::Punct(c) => out.push(*c),
            Token::Break(b) => {
                out.push_str(b.as_str());
                after_break = true;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(s: &str) -> Token {
        Token::Word(s.into())
    }

    #[test]
    fn splits_edge_punctuation_and_keeps_inner() {
        assert_eq!(
            tokenize("Hi ,there 5:30. ok?!"),
            vec![
                w("Hi"),
                Token::Punct(','),
                w("there"),
                w("5:30"),
                Token::Punct('.'),
                w("ok"),
                Token::Punct('?'),
                Token::Punct('!')
            ]
        );
    }

    #[test]
    fn newlines_become_breaks_and_round_trip() {
        let t = tokenize("a\nb\n\nc\n");
        assert_eq!(
            t,
            vec![
                w("a"),
                Token::Break(Break::Line),
                w("b"),
                Token::Break(Break::Paragraph),
                w("c"),
                Token::Break(Break::Line)
            ]
        );
        assert_eq!(render(&t, FormatContext::plain()), "a\nb\n\nc\n");
    }
}
