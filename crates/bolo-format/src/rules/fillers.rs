//! Fillers: hesitation sounds, filler phrases and stutters (ADR-0015).
//!
//! The pass is conservative. It removes only:
//! - Hesitation sounds ("um", "uh", "er"), anywhere.
//! - "you know" between a pause and a pause ("I was, you know, tired"). The pause can be
//!   the start of a sentence or punctuation. "like" counts only between two commas.
//! - A stutter: the same word twice in a row ("the the", "I, I think"). Words people
//!   repeat on purpose ("bye bye", "very very", "that that") are kept, and so are
//!   numbers. In Hinglish only a short list of English words collapses, because Hindi
//!   repeats words for meaning ("jaldi jaldi", "alag alag").
//! - In Hinglish, "matlab", "haan toh", "woh" and "na" only where they can't be real
//!   words: at the start of the dictation, or "matlab matlab". "na" is often "no", so
//!   on its own at the start it stays.

use super::Language;
use super::kinds;
use super::token::Token;

const HESITATIONS: &[&str] = &["um", "umm", "ummm", "uhm", "uh", "uhh", "uhhh", "er", "erm"];

/// Words people repeat on purpose, never collapsed in English.
const MEANT_REPEATS: &[&str] = &[
    "bye",
    "very",
    "no",
    "yes",
    "yeah",
    "so",
    "ha",
    "haha",
    "ho",
    "la",
    "blah",
    "really",
    "that",
    "had",
    "is",
    "knock",
    "tick",
    "tock",
    "well",
    "now",
    "there",
    "go",
    "come",
    "hey",
    "hi",
    "please",
    "more",
    "much",
    "many",
    "far",
    "long",
    "on",
    "over",
    "round",
    "again",
    "never",
    "okay",
    "ok",
    "wait",
    "do",
    "line",
    "paragraph",
];

/// English words that collapse when stuttered inside a Hinglish dictation.
const HINGLISH_STUTTERS: &[&str] =
    &["i", "the", "a", "an", "and", "we", "it", "of", "in", "on", "for", "you", "my", "this", "woh", "wo"];

/// Words that count as a stutter even with a comma between the copies.
const COMMA_STUTTERS: &[&str] = &["i", "i'm", "we", "the", "a", "an", "and", "my", "matlab", "woh", "wo"];

/// Hindi fillers removed from the start of a dictation.
const HINDI_START_FILLERS: &[&str] = &["matlab", "woh", "wo", "na"];

fn lower(t: Option<&Token>) -> Option<String> {
    match t {
        Some(Token::Word(w)) => Some(w.to_lowercase()),
        _ => None,
    }
}

/// A pause before a filler phrase: the start of the text or a line, or punctuation.
fn pause_before(out: &[Token]) -> bool {
    !matches!(out.last(), Some(Token::Word(_)))
}

pub(super) fn apply(tokens: &mut Vec<Token>, language: Language) {
    // Hesitations go first, so the checks below see the words around them.
    tokens.retain(|t| !matches!(t, Token::Word(w) if HESITATIONS.iter().any(|h| w.eq_ignore_ascii_case(h))));
    // One removal can expose another ("i, like like, i": the stutter goes, then "like"
    // sits between commas, then "i, i" is a stutter). The pass only removes tokens, so
    // running it until nothing changes ends, and keeps the pipeline idempotent.
    loop {
        let before = tokens.len();
        remove_once(tokens, language);
        if tokens.len() == before {
            break;
        }
    }
}

fn remove_once(tokens: &mut Vec<Token>, language: Language) {
    let src = std::mem::take(tokens);
    let out = tokens;
    out.reserve(src.len());
    let mut i = 0;
    while i < src.len() {
        let Some(word) = lower(src.get(i)) else {
            out.push(src[i].clone());
            i += 1;
            continue;
        };
        // "you know" between pauses: ", you know," and "You know, …" go; "do you know"
        // and ", you know?" stay.
        if word == "you"
            && lower(src.get(i + 1)).as_deref() == Some("know")
            && pause_before(out)
            && matches!(src.get(i + 2), None | Some(Token::Break(_)) | Some(Token::Punct(',' | '.' | '…')))
        {
            i += 2;
            continue;
        }
        if word == "like" && is_comma(&trailing_punct(out)) && is_comma(&leading_punct(&src[i + 1..])) {
            i += 1;
            continue;
        }
        if language == Language::Hinglish && word == "matlab" && repeat_of(out, &word) {
            // "matlab matlab" is pure hesitation: both go.
            drop_repeat(out);
            out.pop();
            i += 1;
            continue;
        }
        if is_stutter(&word, language) && repeat_of(out, &word) {
            drop_repeat(out);
            i += 1;
            continue;
        }
        out.push(src[i].clone());
        i += 1;
    }
    // Removing one run can expose another ("Matlab. Matlab, kal …").
    while language == Language::Hinglish && hindi_start_fillers(out) {}
}

fn is_stutter(word: &str, language: Language) -> bool {
    match language {
        // Only plain words: numbers, times, codes and URLs repeat on purpose ("4 4 7 2").
        Language::English => {
            !MEANT_REPEATS.contains(&word)
                && !kinds::is_number(word)
                && word.chars().all(|c| c.is_alphabetic() || matches!(c, '\'' | '\u{2019}'))
        }
        Language::Hinglish => HINGLISH_STUTTERS.contains(&word),
    }
}

/// Whether `out` ends with `word`, or with `word` and a comma ("I, I think"). Across a
/// comma only a few words count: "I do, do you?" and "Do it, it works" are not stutters.
/// A run of punctuation the punctuation pass collapses to a comma (",,", ", ;") counts
/// as a comma, so a second run finds nothing more.
fn repeat_of(out: &[Token], word: &str) -> bool {
    let run = trailing_punct(out);
    let prev = &out[..out.len() - run.len()];
    let comma = match run.first() {
        None => false,
        Some(_) if is_comma(&run) => true,
        Some(_) => return false,
    };
    if comma && !COMMA_STUTTERS.contains(&word) {
        return false;
    }
    lower(prev.last()).as_deref() == Some(word)
}

/// Whether a punctuation run reads as a comma once the punctuation pass collapses it
/// (",", ",,", ", ;").
fn is_comma(run: &[char]) -> bool {
    run.first() == Some(&',') && !run.iter().any(|c| matches!(c, '.' | '?' | '!' | '।' | '…'))
}

fn leading_punct(tokens: &[Token]) -> Vec<char> {
    tokens
        .iter()
        .map_while(|t| match t {
            Token::Punct(c) => Some(*c),
            _ => None,
        })
        .collect()
}

fn trailing_punct(out: &[Token]) -> Vec<char> {
    let mut run: Vec<char> = out
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

/// Drop the comma between a word and its repeat; the first copy is kept.
fn drop_repeat(out: &mut Vec<Token>) {
    while matches!(out.last(), Some(Token::Punct(_))) {
        out.pop();
    }
}

/// Remove a run of Hindi fillers at the start of the dictation.
///
/// "matlab" and "haan toh" there are always fillers, so the run up to the last of them
/// goes. A whole run followed by a pause goes too ("Woh, kal …", "Woh na, …"), unless
/// it is only "na", which is usually "no". "Woh movie dekhi?" keeps its "woh".
/// Returns whether anything was removed.
fn hindi_start_fillers(tokens: &mut Vec<Token>) -> bool {
    // Punctuation before the first word is dropped later, so it doesn't hide the start.
    let first = tokens.iter().position(|t| !matches!(t, Token::Punct(_))).unwrap_or(tokens.len());
    let mut i = first;
    let mut end_strong = 0; // after the last "matlab" or "haan toh"
    let mut end_run = 0; // after the last filler word
    let mut only_na = true;
    loop {
        match tokens.get(i) {
            Some(Token::Punct(',' | '…')) if end_run > first => i += 1,
            Some(Token::Word(w)) => {
                let w = w.to_lowercase();
                if w == "haan" && matches!(lower(tokens.get(i + 1)).as_deref(), Some("toh" | "to")) {
                    i += 2;
                    end_strong = i;
                } else if HINDI_START_FILLERS.contains(&w.as_str()) {
                    i += 1;
                    if w == "matlab" {
                        end_strong = i;
                    }
                } else {
                    break;
                }
                end_run = i;
                only_na &= w == "na";
            }
            _ => break,
        }
    }
    let pause_after = matches!(tokens.get(end_run), Some(Token::Punct(',' | '…' | '.' | ';' | ':')));
    let mut end = if pause_after && !only_na { end_run } else { end_strong };
    if end <= first {
        return false;
    }
    while matches!(tokens.get(end), Some(Token::Punct(_))) {
        end += 1;
    }
    // Only fillers: leave the dictation as it is rather than empty it.
    if !matches!(tokens.get(end), Some(Token::Word(_))) {
        return false;
    }
    tokens.drain(..end);
    true
}
