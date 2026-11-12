//! Snippets: a spoken trigger phrase ("my address") that Bolo replaces with saved text.
//!
//! [`expand`] runs last, on text that has already been through Layer 1 or polish, so a
//! trigger may come back changed: capitalised at the start of a sentence, with a final
//! period, with "one" written as "1", or with commas between its words. Matching
//! therefore works on normalised words, not on the text:
//!
//! - A word is a run of letters and digits (any script). Case is ignored, and the number
//!   words zero to twenty equal their digits.
//! - Anything that isn't a word between a trigger's words is ignored ("my, address"), but
//!   a trigger never spans a line break.
//! - Matches are whole words: the text around a match must be separated from it by a
//!   space or the end of the text, so "example" doesn't match inside "example.com".
//! - Each trigger is also matched as Layer 1 would write it said alone, so a trigger
//!   that the rules change ("twenty five" → "25") is still found.
//! - The longest trigger wins where two overlap. The expansion goes in verbatim; it is
//!   never searched for triggers itself.
//!
//! Punctuation around a trigger stays, except that a dictation that is only the trigger
//! becomes exactly the expansion (Layer 1's capital and final period were the rules',
//! not the user's), and a period, question mark or exclamation mark right after a
//! trigger is dropped when the expansion already ends in one.
//!
//! [`expand_html`] does the same on the HTML rendering: tags are line breaks for
//! matching, entities are punctuation, and the expansion is escaped with its line
//! breaks as `<br>`.

use std::ops::Range;

use serde::{Deserialize, Serialize};

use crate::rules::{self, Language};

/// One saved snippet: say `trigger`, get `text`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Snippet {
    /// The phrase the user says, e.g. "my address".
    pub trigger: String,
    /// What replaces it, exactly as written (line breaks included).
    pub text: String,
}

/// Replace every trigger of `snippets` in `text` (plain text or Markdown) with its
/// expansion. Text with no trigger in it comes back unchanged.
pub fn expand(text: &str, snippets: &[Snippet]) -> String {
    Snippets::new(snippets).expand(text)
}

/// [`expand`] for the HTML rendering of a dictation (`rules::to_html` or the HTML of
/// `rules::format`): the expansion is escaped and its line breaks become `<br>`.
pub fn expand_html(html: &str, snippets: &[Snippet]) -> String {
    Snippets::new(snippets).expand_html(html)
}

/// Snippets ready to match, for expanding several texts with the same list.
#[derive(Debug, Clone, Default)]
pub struct Snippets {
    entries: Vec<Entry>,
}

#[derive(Debug, Clone)]
struct Entry {
    trigger: String,
    text: String,
    /// The trigger's word sequences: as typed, and as Layer 1 writes it.
    keys: Vec<Vec<String>>,
}

impl Snippets {
    /// `snippets` compiled for matching. Blank triggers are skipped; of two snippets
    /// with the same trigger words, the first wins.
    pub fn new(snippets: &[Snippet]) -> Self {
        let mut entries: Vec<Entry> = Vec::new();
        for s in snippets {
            let trigger = s.trigger.trim();
            let typed = keys_of(trigger);
            if typed.is_empty() || entries.iter().any(|e| e.keys[0] == typed) {
                continue;
            }
            let mut keys = vec![typed];
            for language in [Language::English, Language::Hinglish] {
                let formatted = keys_of(&rules::format(trigger, language, false).plain);
                if !formatted.is_empty() && !keys.contains(&formatted) {
                    keys.push(formatted);
                }
            }
            entries.push(Entry { trigger: trigger.to_owned(), text: s.text.clone(), keys });
        }
        Self { entries }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// See [`expand`].
    pub fn expand(&self, text: &str) -> String {
        self.replace(text, false)
    }

    /// See [`expand_html`].
    pub fn expand_html(&self, html: &str) -> String {
        self.replace(html, true)
    }

    /// The triggers said in `text`, in the order they appear, each once: what polish is
    /// told to leave as spoken.
    pub fn triggers_in(&self, text: &str) -> Vec<&str> {
        let mut found: Vec<&str> = Vec::new();
        for m in self.matches(&words(text, false), text) {
            let t = self.entries[m.entry].trigger.as_str();
            if !found.contains(&t) {
                found.push(t);
            }
        }
        found
    }

    fn replace(&self, text: &str, html: bool) -> String {
        if self.entries.is_empty() {
            return text.to_owned();
        }
        let words = words(text, html);
        let matches = self.matches(&words, text);
        if matches.is_empty() {
            return text.to_owned();
        }
        let render = |m: &Match| {
            let expansion = &self.entries[m.entry].text;
            if html { html_text(expansion) } else { expansion.clone() }
        };
        // A dictation that is only the trigger becomes only the expansion (in HTML, the
        // tags around it stay).
        if let [m] = matches.as_slice()
            && m.words == (0..words.len())
        {
            let span = words[0].span.start..words[words.len() - 1].span.end;
            let mut out = if html { tags(&text[..span.start]) } else { String::new() };
            out.push_str(&render(m));
            if html {
                out.push_str(&tags(&text[span.end..]));
            }
            return out;
        }
        let mut out = String::with_capacity(text.len());
        let mut at = 0;
        for m in &matches {
            let span = words[m.words.start].span.start..words[m.words.end - 1].span.end;
            out.push_str(&text[at..span.start]);
            out.push_str(&render(m));
            at = span.end;
            if self.entries[m.entry].text.trim_end().ends_with(SENTENCE_END) {
                at += text[at..].len() - text[at..].trim_start_matches(SENTENCE_END).len();
            }
        }
        out.push_str(&text[at..]);
        out
    }

    /// Every trigger in `words`, longest first where two overlap, in text order.
    fn matches(&self, words: &[Word], text: &str) -> Vec<Match> {
        let mut found: Vec<Match> = Vec::new();
        for start in 0..words.len() {
            for (entry, e) in self.entries.iter().enumerate() {
                for key in &e.keys {
                    let end = start + key.len();
                    if end <= words.len()
                        && words[start..end].iter().zip(key).all(|(w, k)| w.key == *k && w.line == words[start].line)
                        && whole(words, start..end, text)
                    {
                        found.push(Match { words: start..end, entry });
                    }
                }
            }
        }
        found.sort_by_key(|m| (std::cmp::Reverse(m.words.len()), m.words.start, m.entry));
        let mut taken = vec![false; words.len()];
        let mut kept: Vec<Match> = Vec::new();
        for m in found {
            if taken[m.words.clone()].iter().any(|t| *t) {
                continue;
            }
            taken[m.words.clone()].iter_mut().for_each(|t| *t = true);
            kept.push(m);
        }
        kept.sort_by_key(|m| m.words.start);
        kept
    }
}

const SENTENCE_END: [char; 3] = ['.', '!', '?'];

struct Match {
    words: Range<usize>,
    entry: usize,
}

/// One word of the text being searched.
#[derive(Debug)]
struct Word {
    /// Its bytes in the text.
    span: Range<usize>,
    /// Normalised: lower case, number words as digits.
    key: String,
    /// Which line it is on (a tag counts as a line break in HTML).
    line: usize,
}

/// The words of `text`. In `html`, tags break lines and entities are punctuation.
fn words(text: &str, html: bool) -> Vec<Word> {
    let mut out = Vec::new();
    let mut line = 0;
    let mut start: Option<usize> = None;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if is_word_char(c) {
            start.get_or_insert(i);
            continue;
        }
        if let Some(s) = start.take() {
            out.push(Word { span: s..i, key: normalise(&text[s..i]), line });
        }
        match c {
            '\n' => line += 1,
            '<' if html => {
                line += 1;
                for (_, c) in chars.by_ref() {
                    if c == '>' {
                        break;
                    }
                }
            }
            '&' if html => {
                let rest = &text[i + 1..];
                if let Some(end) = rest
                    .find(';')
                    .filter(|&e| e <= 8 && rest[..e].chars().all(|c| c.is_ascii_alphanumeric() || c == '#'))
                {
                    for _ in 0..rest[..end].chars().count() + 1 {
                        chars.next();
                    }
                }
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        out.push(Word { span: s..text.len(), key: normalise(&text[s..]), line });
    }
    out
}

/// A trigger's normalised words.
fn keys_of(trigger: &str) -> Vec<String> {
    words(trigger, false).into_iter().map(|w| w.key).collect()
}

/// `words[range]` stands apart from the words around it: whatever separates it from
/// the word before and the word after (on the same line) includes a space.
fn whole(words: &[Word], range: Range<usize>, text: &str) -> bool {
    let first = &words[range.start];
    let last = &words[range.end - 1];
    let apart =
        |a: &Word, b: &Word| a.line != b.line || text[a.span.end..b.span.start].chars().any(char::is_whitespace);
    let before = range.start == 0 || apart(&words[range.start - 1], first);
    let after = range.end == words.len() || apart(last, &words[range.end]);
    before && after
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || !(c.is_ascii() || c.is_whitespace() || is_punctuation(c))
}

/// Non-ASCII punctuation: Latin-1 symbols, general punctuation (quotes, dashes, the
/// ellipsis), CJK punctuation and the Devanagari danda.
fn is_punctuation(c: char) -> bool {
    matches!(c, '\u{A1}'..='\u{BF}' | '\u{D7}' | '\u{F7}' | '\u{2000}'..='\u{206F}' | '\u{3000}'..='\u{303F}' | '।' | '॥')
}

const NUMBER_WORDS: [&str; 21] = [
    "zero",
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
    "eleven",
    "twelve",
    "thirteen",
    "fourteen",
    "fifteen",
    "sixteen",
    "seventeen",
    "eighteen",
    "nineteen",
    "twenty",
];

fn normalise(word: &str) -> String {
    let lower = word.to_lowercase();
    match NUMBER_WORDS.iter().position(|w| *w == lower) {
        Some(n) => n.to_string(),
        None => lower,
    }
}

/// Only the tags of an HTML fragment.
fn tags(html: &str) -> String {
    let mut out = String::new();
    let mut rest = html;
    while let Some(open) = rest.find('<') {
        let Some(close) = rest[open..].find('>') else { break };
        out.push_str(&rest[open..open + close + 1]);
        rest = &rest[open + close + 1..];
    }
    out
}

/// An expansion as HTML text: escaped, line breaks as `<br>`.
fn html_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            out.push_str("<br>");
        }
        rules::escape_html(line.strip_suffix('\r').unwrap_or(line), &mut out);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(trigger: &str, text: &str) -> Snippet {
        Snippet { trigger: trigger.into(), text: text.into() }
    }

    #[test]
    fn words_skip_tags_and_entities_in_html() {
        let w = words("<p>Tom&amp;Jerry&#39;s<br>dog</p>", true);
        let keys: Vec<&str> = w.iter().map(|w| w.key.as_str()).collect();
        assert_eq!(keys, ["tom", "jerry", "s", "dog"]);
        assert_eq!(w[2].line, w[1].line);
        assert_ne!(w[3].line, w[2].line, "a <br> is a line break");
    }

    #[test]
    fn triggers_in_lists_each_trigger_once() {
        let list = Snippets::new(&[s("sig", "—N"), s("my address", "x"), s("", "blank")]);
        assert_eq!(list.triggers_in("Sig. Send it to my address, sig."), ["sig", "my address"]);
        assert!(list.triggers_in("nothing here").is_empty());
    }

    #[test]
    fn duplicate_triggers_keep_the_first() {
        assert_eq!(expand("sig", &[s("Sig", "first"), s(" sig ", "second")]), "first");
    }
}
