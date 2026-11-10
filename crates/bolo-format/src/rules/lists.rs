//! Spoken lists: an explicit enumeration of at least three items becomes a numbered list
//! (ADR-0015). The enumerations are:
//!
//! - ordinals: "first … second … third" (also "firstly", "secondly", …);
//! - cardinals: "one … two … three", each at the start of a clause or set off by a
//!   comma after it ("…, two, eggs");
//! - "number one … number two … number three".
//!
//! All three are numbered speech, so every list renders as "1. …", one item per line.
//! Text before the first marker in the same sentence ("I need three things") is the
//! lead-in and becomes the intro line, ending in a colon.
//!
//! When unsure, the text stays prose:
//! - two items are not a list, and prose joined with "and" never is;
//! - a marker after a determiner ("the first", "my number one") or before "of" ("first
//!   of all") doesn't count;
//! - a cardinal counts only at a clause start, so "one apple, two pears, three plums"
//!   stays prose;
//! - an item with no words cancels the list.
//!
//! The markers, and an "and", "then" or "aur" that joins the items, are the only words the pass
//! removes. A rendered list ("1. Milk" on its own line) reads back as the same list, so
//! the pipeline stays idempotent.
//!
//! Lists are English enumerations only; Hindi ordinals ("pehla", "doosra") are left to
//! polish.

use super::token::{Break, Token, push_break};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Family {
    Ordinal,
    Cardinal,
    Number,
    /// A list rendered earlier: "N." at the start of a line. Only this form continues
    /// it, so words inside an item never read as a marker.
    Numeral,
}

/// Words that make a following marker part of a noun phrase: "the first", "my number one".
const DETERMINERS: &[&str] = &[
    "a", "an", "the", "this", "that", "these", "those", "my", "your", "our", "their", "his", "her", "its", "every",
    "each", "at", "per", "one's",
];

/// Words that join the last items ("…, and third bread"); dropped from the end of an item.
const JOINERS: &[&str] = &["and", "then", "aur"];

const ORDINALS: &[&str] = &[
    "first",
    "second",
    "third",
    "fourth",
    "fifth",
    "sixth",
    "seventh",
    "eighth",
    "ninth",
    "tenth",
    "eleventh",
    "twelfth",
    "thirteenth",
    "fourteenth",
    "fifteenth",
    "sixteenth",
    "seventeenth",
    "eighteenth",
    "nineteenth",
    "twentieth",
];

const CARDINALS: &[&str] = &[
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

fn position(table: &[&str], w: &str) -> Option<u32> {
    table.iter().position(|x| *x == w).map(|p| p as u32 + 1)
}

/// "first" / "firstly" → 1.
fn ordinal_value(w: &str) -> Option<u32> {
    position(ORDINALS, w).or_else(|| w.strip_suffix("ly").and_then(|s| position(ORDINALS, s)))
}

/// "three" / "3" → 3. Digits come from the engine ("10") or from a list
/// rendered earlier ("3." on its own line).
fn cardinal_value(w: &str) -> Option<u32> {
    if let Some(n) = position(CARDINALS, w) {
        return Some(n);
    }
    if w.len() <= 2 && !w.starts_with('0') && w.chars().all(|c| c.is_ascii_digit()) {
        return w.parse().ok();
    }
    None
}

fn lower(t: Option<&Token>) -> Option<String> {
    match t {
        Some(Token::Word(w)) => Some(w.to_lowercase()),
        _ => None,
    }
}

fn is_sentence_end(t: &Token) -> bool {
    matches!(t, Token::Punct('.' | '!' | '?' | '।'))
}

/// A marker isn't taken after a determiner.
fn after_determiner(tokens: &[Token], i: usize) -> bool {
    i > 0 && lower(tokens.get(i - 1)).is_some_and(|w| DETERMINERS.contains(&w.as_str()))
}

/// The start of a clause: the text or a line starts, or a sentence or a colon ends.
fn clause_start(tokens: &[Token], i: usize) -> bool {
    i == 0
        || matches!(tokens[i - 1], Token::Break(_) | Token::Item(_) | Token::Punct(':'))
        || is_sentence_end(&tokens[i - 1])
}

/// Like [`clause_start`], but also after a comma or a joining word.
fn loose_start(tokens: &[Token], i: usize) -> bool {
    clause_start(tokens, i)
        || matches!(tokens[i - 1], Token::Punct(_))
        || lower(tokens.get(i - 1)).is_some_and(|w| JOINERS.contains(&w.as_str()))
}

/// Marker `n` of `family` (any family when `None`) at `tokens[i]`, and how many tokens
/// it spans.
fn marker_at(tokens: &[Token], i: usize, family: Option<Family>, n: u32) -> Option<(Family, usize)> {
    let w = lower(tokens.get(i))?;
    if after_determiner(tokens, i) {
        return None;
    }
    let wants = |f: Family| family.is_none_or(|x| x == f);
    let next = tokens.get(i + 1);
    let line_start = i == 0 || matches!(tokens[i - 1], Token::Break(_) | Token::Item(_));
    if w.starts_with(|c: char| c.is_ascii_digit()) && line_start && matches!(next, Some(Token::Punct('.'))) {
        return (wants(Family::Numeral) && cardinal_value(&w) == Some(n)).then_some((Family::Numeral, 1));
    }
    if wants(Family::Number) && w == "number" && lower(next).and_then(|x| cardinal_value(&x)) == Some(n) {
        return Some((Family::Number, 2));
    }
    if wants(Family::Ordinal) && ordinal_value(&w) == Some(n) && lower(next).is_none_or(|x| x != "of") {
        return Some((Family::Ordinal, 1));
    }
    if wants(Family::Cardinal)
        && cardinal_value(&w) == Some(n)
        && (clause_start(tokens, i) || (loose_start(tokens, i) && matches!(next, Some(Token::Punct(',' | ':' | '.')))))
    {
        return Some((Family::Cardinal, 1));
    }
    None
}

/// One list item: the token range of its content (after the marker).
struct Item {
    start: usize,
    end: usize,
}

struct List {
    /// Where the lead-in starts (equal to `first_marker` when there is none).
    lead_in: usize,
    first_marker: usize,
    items: Vec<Item>,
    /// One past the last token the list covers.
    end: usize,
}

/// Where the last item, starting at `start`, ends: at the end of its sentence, or at a
/// break. A mark before its first word ("3. Bread") doesn't end it.
fn last_item_end(tokens: &[Token], start: usize) -> usize {
    let mut k = start;
    let mut has_word = false;
    while k < tokens.len() && !matches!(tokens[k], Token::Break(_) | Token::Item(_)) {
        has_word |= matches!(tokens[k], Token::Word(_));
        k += 1;
        if has_word && is_sentence_end(&tokens[k - 1]) {
            // The whole closing run ("..", "?!") belongs to the item.
            while matches!(tokens.get(k), Some(Token::Punct(_))) {
                k += 1;
            }
            break;
        }
    }
    k
}

/// The list whose first marker is at `tokens[at]`, if one is there.
fn list_at(tokens: &[Token], at: usize) -> Option<List> {
    let (family, len) = marker_at(tokens, at, None, 1)?;
    let mut items = Vec::new();
    let mut start = at + len;
    let mut n = 1;
    loop {
        // Look for the next marker. A break ends the search, unless the marker follows
        // it (after any stray marks a removed filler left behind).
        let mut next = None;
        let mut j = start;
        while j < tokens.len() {
            match tokens[j] {
                Token::Item(_) => break,
                Token::Break(_) => {
                    let k =
                        (j + 1..tokens.len()).find(|&k| !matches!(tokens[k], Token::Punct(_))).unwrap_or(tokens.len());
                    if let Some((_, l)) = marker_at(tokens, k, Some(family), n + 1) {
                        next = Some((j, k, l));
                    }
                    break;
                }
                _ => {
                    if let Some((_, l)) = marker_at(tokens, j, Some(family), n + 1) {
                        next = Some((j, j, l));
                        break;
                    }
                }
            }
            j += 1;
        }
        let Some((content_end, marker, l)) = next else { break };
        items.push(Item { start, end: content_end });
        start = marker + l;
        n += 1;
    }
    items.push(Item { start, end: last_item_end(tokens, start) });
    let has_word = |it: &Item| tokens[it.start..it.end].iter().any(|t| matches!(t, Token::Word(_)));
    // A last "marker" with nothing after it ("…, three, four.") is a word of the item
    // before it, which is then the last item.
    if items.len() > 3 && items.last().is_some_and(|it| !has_word(it)) {
        items.pop();
        if let Some(it) = items.last_mut() {
            it.end = last_item_end(tokens, it.start);
        }
    }
    if items.len() < 3 || !items.iter().all(has_word) {
        return None;
    }
    let end = items.last().map_or(at, |it| it.end);
    let lead_in = tokens[..at]
        .iter()
        .rposition(|t| matches!(t, Token::Break(_) | Token::Item(_)) || is_sentence_end(t))
        .map_or(0, |p| p + 1);
    Some(List { lead_in, first_marker: at, items, end })
}

/// An item's content, without punctuation at its start and without a trailing comma,
/// semicolon, colon or joining word. Short items (`sentences` false) also lose a trailing
/// period, and then anything of the above before it.
fn trimmed(content: &[Token], sentences: bool) -> Vec<Token> {
    let mut v: Vec<Token> = content.iter().skip_while(|t| matches!(t, Token::Punct(_))).cloned().collect();
    loop {
        match v.last() {
            Some(Token::Punct(',' | ';' | ':')) => {}
            Some(Token::Punct('.' | '…')) if !sentences => {}
            Some(Token::Word(w)) if JOINERS.contains(&w.to_lowercase().as_str()) && v.len() > 1 => {}
            _ => break,
        }
        v.pop();
    }
    v
}

/// The list as tokens: intro line, items, and the breaks around them.
fn render_list(tokens: &[Token], list: &List, out: &mut Vec<Token>) {
    let mut intro: Vec<Token> = tokens[list.lead_in..list.first_marker].to_vec();
    while matches!(intro.last(), Some(Token::Punct(_))) {
        intro.pop();
    }
    if !intro.is_empty() {
        out.extend(intro);
        out.push(Token::Punct(':'));
    }
    if !out.is_empty() {
        push_break(out, Break::List);
    }
    // Sentence items ("Update the dependencies.") all end in a sentence mark and the last
    // gets one too; short items ("Milk") end in none.
    let content = |it: &Item| &tokens[it.start..it.end];
    let (last, rest) = list.items.split_last().expect("a list has items");
    let sentences = rest.iter().all(|it| trimmed(content(it), true).last().is_some_and(is_sentence_end));
    let mut items: Vec<Vec<Token>> = rest.iter().map(|it| trimmed(content(it), sentences)).collect();
    let mut last = trimmed(content(last), sentences);
    if sentences && matches!(last.last(), Some(Token::Word(_))) {
        last.push(Token::Punct('.'));
    }
    items.push(last);
    for (n, item) in items.into_iter().enumerate() {
        out.push(Token::Item(n as u32 + 1));
        out.extend(item);
    }
}

pub(super) fn apply(tokens: &mut Vec<Token>) {
    let mut i = 0;
    while i < tokens.len() {
        let Some(list) = list_at(tokens, i) else {
            i += 1;
            continue;
        };
        let mut out: Vec<Token> = tokens[..list.lead_in].to_vec();
        render_list(tokens, &list, &mut out);
        let rest = &tokens[list.end..];
        if !rest.is_empty() {
            push_break(&mut out, Break::List);
        }
        i = out.len();
        for t in rest {
            match t {
                Token::Break(b) if matches!(out.last(), Some(Token::Break(_))) => push_break(&mut out, *b),
                t => out.push(t.clone()),
            }
        }
        *tokens = out;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_values() {
        assert_eq!(ordinal_value("first"), Some(1));
        assert_eq!(ordinal_value("thirdly"), Some(3));
        assert_eq!(ordinal_value("only"), None);
        assert_eq!(cardinal_value("twelve"), Some(12));
        assert_eq!(cardinal_value("12"), Some(12));
        assert_eq!(cardinal_value("05"), None);
        assert_eq!(cardinal_value("123"), None);
    }
}
