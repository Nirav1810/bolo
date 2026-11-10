//! Punctuation fix-ups: sentence-start capitals, a final period, a capital "I", clean
//! spacing (rebuilt at render) and no doubled punctuation. The pass never adds commas;
//! missing commas are polish's job (ADR-0015).
//!
//! `code_mode` targets get no automatic capitals and no final period: a period the
//! engine put at the end is removed, and a capitalized first word is lowercased.

use super::FormatContext;
use super::token::Token;

const ELLIPSIS: &[char] = &['.', '.', '.'];

fn is_devanagari(c: char) -> bool {
    ('\u{0900}'..='\u{097F}').contains(&c)
}

/// A punctuation run that ends a sentence (an ellipsis doesn't).
fn ends_sentence(run: &[char]) -> bool {
    matches!(run, ['.' | '!' | '?' | '।'])
}

/// One mark for a run of several: "?." → "?", ",," → ",", ", ." → "."; an ellipsis stays.
fn collapse(run: &[char]) -> Vec<char> {
    if run.len() <= 1 || run == ELLIPSIS || run == ['…'] {
        return run.to_vec();
    }
    if run.iter().all(|&c| c == '.' || c == '…') {
        return if run.len() >= 3 || run.contains(&'…') { ELLIPSIS.to_vec() } else { vec!['.'] };
    }
    let strongest = ['?', '!', '।', '.'].into_iter().find(|c| run.contains(c));
    vec![strongest.unwrap_or(run[0])]
}

/// Collapse punctuation runs, and drop any at the start of the text or of a line.
fn collapse_runs(tokens: &mut Vec<Token>) {
    let src = std::mem::take(tokens);
    let mut run = Vec::new();
    for t in src {
        if let Token::Punct(c) = t {
            run.push(c);
            continue;
        }
        flush_run(tokens, &mut run);
        tokens.push(t);
    }
    flush_run(tokens, &mut run);
}

fn flush_run(tokens: &mut Vec<Token>, run: &mut Vec<char>) {
    if !run.is_empty() && matches!(tokens.last(), Some(Token::Word(_))) {
        tokens.extend(collapse(run).into_iter().map(Token::Punct));
    }
    run.clear();
}

/// "i", "i'm", "i'll", "i've", "i'd" → capital I.
fn capital_i(word: &mut String) {
    let is_i = word == "i"
        || ["'", "\u{2019}"].iter().any(|apos| {
            word.strip_prefix('i')
                .and_then(|r| r.strip_prefix(apos))
                .is_some_and(|r| matches!(r, "m" | "ll" | "ve" | "d"))
        });
    if is_i {
        word.replace_range(..1, "I");
    }
}

/// Whether a sentence-start capital is safe: an all-lowercase word, not a URL, email,
/// path or handle ("example.com", "iPhone" and "5pm" are left alone).
fn can_capitalize(word: &str) -> bool {
    word.chars().next().is_some_and(char::is_lowercase)
        && !word.chars().any(|c| c.is_uppercase() || matches!(c, '.' | '/' | '\\' | '@' | '_'))
}

fn capitalize(word: &mut String) {
    if let Some(first) = word.chars().next() {
        let upper: String = first.to_uppercase().collect();
        word.replace_range(..first.len_utf8(), &upper);
    }
}

fn sentence_capitals(tokens: &mut [Token]) {
    let mut at_start = true;
    let mut run = Vec::new();
    for t in tokens.iter_mut() {
        match t {
            Token::Word(w) => {
                if (at_start || ends_sentence(&run)) && can_capitalize(w) {
                    capitalize(w);
                }
                at_start = false;
                run.clear();
            }
            Token::Punct(c) => run.push(*c),
            Token::Break(_) => {
                at_start = true;
                run.clear();
            }
        }
    }
}

/// The punctuation run at the end, if the text ends on punctuation.
fn trailing_run(tokens: &[Token]) -> Vec<char> {
    let mut run: Vec<char> = tokens
        .iter()
        .rev()
        .map_while(|t| match t {
            Token::Punct(c) => Some(*c),
            _ => None,
        })
        .collect();
    run.reverse();
    run
}

fn final_period(tokens: &mut Vec<Token>) {
    let run = trailing_run(tokens);
    let Some(Token::Word(last_word)) = tokens.iter().rev().nth(run.len()) else {
        return; // ends on a break: the user is laying out, not ending a sentence
    };
    let period = if last_word.chars().last().is_some_and(is_devanagari) { '।' } else { '.' };
    match run.as_slice() {
        [] => tokens.push(Token::Punct(period)),
        [',' | ';' | ':'] => {
            tokens.pop();
            tokens.push(Token::Punct(period));
        }
        _ => {}
    }
}

fn code_mode(tokens: &mut Vec<Token>) {
    if matches!(trailing_run(tokens).as_slice(), ['.' | '।']) {
        tokens.pop();
    }
    if let Some(Token::Word(w)) = tokens.first_mut() {
        let mut chars = w.chars();
        let titlecase = chars.next().is_some_and(char::is_uppercase)
            && !chars.as_str().is_empty()
            && !chars.as_str().chars().any(char::is_uppercase)
            && !chars.as_str().starts_with(['\'', '\u{2019}']);
        if titlecase {
            let first = w.chars().next().unwrap_or_default();
            let lower: String = first.to_lowercase().collect();
            w.replace_range(..first.len_utf8(), &lower);
        }
    }
}

pub(super) fn apply(tokens: &mut Vec<Token>, ctx: FormatContext) {
    collapse_runs(tokens);
    if ctx.code_mode {
        code_mode(tokens);
        return;
    }
    for t in tokens.iter_mut() {
        if let Token::Word(w) = t {
            capital_i(w);
        }
    }
    sentence_capitals(tokens);
    final_period(tokens);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapse_keeps_the_strongest_mark() {
        assert_eq!(collapse(&['?', '.']), vec!['?']);
        assert_eq!(collapse(&[',', ',']), vec![',']);
        assert_eq!(collapse(&[',', '.']), vec!['.']);
        assert_eq!(collapse(&['.', '.']), vec!['.']);
        assert_eq!(collapse(&['.', '.', '.', '.']), ELLIPSIS.to_vec());
        assert_eq!(collapse(&[':', ',']), vec![':']);
    }
}
