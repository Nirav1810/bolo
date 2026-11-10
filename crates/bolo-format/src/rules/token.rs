//! Tokens the passes work on, and rendering them back to text.
//!
//! Text is split on whitespace. Sentence punctuation at the edges of a chunk becomes its
//! own token, so spacing is rebuilt from scratch at render time. Punctuation inside a
//! chunk stays put ("5:30", "3.5", "example.com", "1,000"). Newlines in the input become
//! line or paragraph breaks, so rendered output tokenizes back to the same tokens.
//!
//! The lists pass adds list items and the breaks around a list. Plain text puts a list on
//! its own lines; Markdown also leaves a blank line before and after it, so that the list
//! is not read as part of the paragraph around it. HTML puts paragraphs in `<p>`, line
//! breaks as `<br>` and a list in `<ol><li>`, and escapes every text node.

use super::Markup;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Token {
    Word(String),
    Punct(char),
    Break(Break),
    /// The start of list item `n`: rendered "n. " on a new line.
    Item(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Break {
    Line,
    /// Before or after a list: a line break in plain text, a blank line in Markdown.
    List,
    Paragraph,
}

impl Break {
    fn as_str(self, markup: Markup) -> &'static str {
        match (self, markup) {
            (Self::Line, _) | (Self::List, Markup::Plain) => "\n",
            (Self::Paragraph, _) | (Self::List, Markup::Markdown | Markup::Html) => "\n\n",
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
pub(super) fn render(tokens: &[Token], markup: Markup) -> String {
    if markup == Markup::Html {
        return render_html(tokens);
    }
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
                out.push_str(b.as_str(markup));
                after_break = true;
            }
            Token::Item(n) => {
                if !out.is_empty() && !out.ends_with('\n') {
                    out.push('\n');
                }
                out.push_str(&format!("{n}."));
                after_break = false;
            }
        }
    }
    out
}

/// The HTML block being written.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Block {
    None,
    Paragraph,
    List,
}

/// HTML being written, one block at a time.
struct Html {
    out: String,
    block: Block,
    /// A line break inside the paragraph, written only if more text follows in it.
    pending_br: bool,
}

impl Html {
    /// Open a paragraph for inline text, or write the line break waiting in it.
    fn inline(&mut self) {
        match self.block {
            Block::None => {
                self.out.push_str("<p>");
                self.block = Block::Paragraph;
            }
            Block::Paragraph if self.pending_br => self.out.push_str("<br>"),
            _ => {}
        }
        self.pending_br = false;
    }

    /// Start list item `n`: a new `<li>` in the open list, or a new `<ol>`.
    fn item(&mut self, n: u32) {
        if self.block == Block::List {
            self.out.push_str("</li><li>");
        } else {
            self.close();
            if n == 1 {
                self.out.push_str("<ol><li>");
            } else {
                self.out.push_str(&format!("<ol start=\"{n}\"><li>"));
            }
            self.block = Block::List;
        }
    }

    fn close(&mut self) {
        match self.block {
            Block::Paragraph => self.out.push_str("</p>"),
            Block::List => self.out.push_str("</li></ol>"),
            Block::None => {}
        }
        self.block = Block::None;
        self.pending_br = false;
    }
}

/// Join tokens into HTML, spaced like [`render`]: inline text in `<p>`, a line break as
/// `<br>` (between text only, never at a paragraph's edge), any other break closing the
/// block, and list items in one `<ol>` until a break that no item follows.
fn render_html(tokens: &[Token]) -> String {
    let mut h = Html { out: String::with_capacity(tokens.len() * 8 + 16), block: Block::None, pending_br: false };
    let mut after_break = true;
    for (i, t) in tokens.iter().enumerate() {
        match t {
            Token::Word(w) => {
                h.inline();
                if !after_break {
                    h.out.push(' ');
                }
                escape_html(w, &mut h.out);
                after_break = false;
            }
            Token::Punct(c) => {
                h.inline();
                escape_html(c.encode_utf8(&mut [0; 4]), &mut h.out);
            }
            Token::Break(b) => {
                after_break = true;
                match h.block {
                    Block::Paragraph if *b == Break::Line => h.pending_br = true,
                    Block::List if matches!(tokens.get(i + 1), Some(Token::Item(_))) => {}
                    _ => h.close(),
                }
            }
            Token::Item(n) => {
                h.item(*n);
                after_break = true;
            }
        }
    }
    h.close();
    h.out
}

/// Append `text` to `out` with the HTML special characters escaped.
pub(super) fn escape_html(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
}

/// Read lines of already formatted text that start "n. " (n up to three digits, then a
/// word) as list items, the way [`render`] writes them.
pub(super) fn number_lines_to_items(tokens: &mut Vec<Token>) {
    let mut i = 0;
    while i + 2 < tokens.len() {
        let line_start = i == 0 || matches!(tokens[i - 1], Token::Break(_));
        let item = match (&tokens[i], &tokens[i + 1], &tokens[i + 2]) {
            (Token::Word(n), Token::Punct('.'), Token::Word(_))
                if line_start && n.len() <= 3 && n.bytes().all(|b| b.is_ascii_digit()) =>
            {
                n.parse().ok()
            }
            _ => None,
        };
        if let Some(n) = item {
            tokens.splice(i..i + 2, [Token::Item(n)]);
        }
        i += 1;
    }
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
        assert_eq!(render(&t, Markup::Plain), "a\nb\n\nc\n");
    }
}
