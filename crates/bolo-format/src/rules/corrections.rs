//! Spoken self-corrections (ADR-0015): "Tuesday, actually no, Friday" → "Friday".
//!
//! The pass acts only on explicit markers: "actually no", "I mean", "sorry",
//! "scratch that", "nahi nahi" and "nahi wait". The words after a marker are the
//! **repair**. A repair replaces the nearest preceding phrase of the same kind, searched
//! in the marker's own sentence, or in the sentence before when the marker starts a new
//! one (engines often write "Tuesday. Actually, no. Make it Friday."):
//!
//! - A date, time or number repair replaces the nearest date, time or number. If the
//!   repair restates the rest of the clause ("kal shaam ko chalte hain, nahi wait,
//!   parson chalte hain"), the clause goes too.
//! - Any other repair is a noun phrase. It replaces the same number of words just before
//!   the marker, and only when both sides look like noun phrases: English, at most three
//!   words, no pronouns, verbs or prepositions, and a shared word when longer than one
//!   ("the red one, I mean the blue one"). "sorry" never corrects a noun phrase, because
//!   "I was wrong, sorry John" is an apology.
//! - "scratch that" with no matching date, time or number drops the sentence before it,
//!   when a pause sets it off ("Send it to John. Scratch that. Send it to Priya.").
//!
//! Anything else stays verbatim: "I mean it", "sorry for the delay", and a repair with
//! nothing of its kind to replace.

use super::Language;
use super::kinds::{self, Kind};
use std::ops::Range;

use super::token::Token;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Marker {
    /// Corrects dates, times, numbers and noun phrases.
    Strong,
    /// "sorry": dates, times and numbers only.
    Sorry,
    /// "scratch that": dates, times and numbers, and otherwise drops the sentence before it.
    Scratch,
}

const MARKERS: &[(&[&str], Marker)] = &[
    (&["actually", "no"], Marker::Strong),
    (&["i", "mean"], Marker::Strong),
    (&["nahi", "nahi"], Marker::Strong),
    (&["nahin", "nahin"], Marker::Strong),
    (&["nahi", "nahin"], Marker::Strong),
    (&["nahin", "nahi"], Marker::Strong),
    (&["nahi", "wait"], Marker::Strong),
    (&["nahin", "wait"], Marker::Strong),
    (&["scratch", "that"], Marker::Scratch),
    (&["sorry"], Marker::Sorry),
];

/// Words before "sorry" that make it an apology ("I'm sorry", "so sorry").
const APOLOGY: &[&str] = &[
    "i'm", "im", "am", "so", "very", "really", "feel", "be", "was", "were", "are", "is", "not", "say", "said", "we're",
    "you're", "he's", "she's", "they're", "i", "we", "terribly", "truly",
];

/// Words before "I mean" that make it literal ("what I mean", "you know what I mean").
const LITERAL_I_MEAN: &[&str] = &["what", "if", "that", "do", "don't", "did", "didn't", "really", "how"];

/// Words after "scratch that" that make it a verb ("scratch that off the list").
const SCRATCH_OBJECTS: &[&str] = &["off", "out", "from", "itch", "part", "bit", "one", "line", "last"];

/// Words a repair may start with before the phrase it replaces with ("make it Friday").
const REPAIR_LEADS: &[&[&str]] =
    &[&["make", "it"], &["make", "that"], &["it's"], &["it", "is"], &["let's", "say"], &["rather"]];

const DETERMINERS: &[&str] =
    &["the", "a", "an", "this", "that", "these", "those", "my", "your", "our", "their", "his", "her"];

/// Words that are not part of a noun phrase: pronouns, prepositions, conjunctions,
/// auxiliaries, common verbs and adverbs, greetings.
const NOT_NOUN: &[&str] = &[
    "i", "i'm", "i'll", "i've", "i'd", "me", "my", "you", "your", "you're", "you'll", "he", "he's", "she", "she's",
    "it", "it's", "its", "we", "we're", "we'll", "they", "they're", "them", "him", "her", "us", "our", "their", "this",
    "that", "these", "those", "there", "there's", "here", "what", "who", "which", "when", "where", "why", "how", "to",
    "for", "of", "in", "on", "at", "by", "with", "from", "about", "into", "over", "after", "before", "until", "than",
    "as", "like", "and", "or", "but", "so", "because", "if", "then", "is", "are", "was", "were", "be", "been", "being",
    "am", "do", "does", "did", "have", "has", "had", "will", "would", "can", "could", "should", "shall", "may",
    "might", "must", "not", "don't", "can't", "won't", "isn't", "wasn't", "didn't", "doesn't", "get", "got", "go",
    "going", "make", "let's", "let", "mean", "meant", "know", "think", "need", "want", "said", "say", "very", "really",
    "too", "also", "just", "actually", "no", "yes", "yeah", "okay", "ok", "please", "well", "still", "even", "again",
    "only", "much", "thanks", "thank", "hi", "hello", "hey", "bye", "sorry", "wait", "right", "sure",
];

fn word_at(tokens: &[Token], i: usize) -> Option<String> {
    match tokens.get(i) {
        Some(Token::Word(w)) => Some(w.to_lowercase()),
        _ => None,
    }
}

/// Lowercase words of `tokens[range]`, which must be all words.
fn words(tokens: &[Token]) -> Vec<String> {
    tokens.iter().filter_map(|t| word_at(std::slice::from_ref(t), 0)).collect()
}

/// The marker starting at `tokens[i]`, and the index just after its last word.
/// Punctuation between the marker's words is skipped ("Actually, no.").
fn marker_at(tokens: &[Token], i: usize) -> Option<(Marker, usize)> {
    'markers: for (phrase, marker) in MARKERS {
        let mut j = i;
        for (n, want) in phrase.iter().enumerate() {
            if n > 0 {
                while matches!(tokens.get(j), Some(Token::Punct(_))) {
                    j += 1;
                }
            }
            if !matches!(tokens.get(j), Some(Token::Word(w)) if w.eq_ignore_ascii_case(want)) {
                continue 'markers;
            }
            j += 1;
        }
        return Some((*marker, j));
    }
    None
}

fn ends_sentence(t: &Token) -> bool {
    matches!(t, Token::Punct('.' | '!' | '?' | '।') | Token::Break(_))
}

/// Index just after the end of the sentence before `end` (0 at the start of the text).
fn sentence_start(tokens: &[Token], end: usize) -> usize {
    tokens[..end].iter().rposition(ends_sentence).map_or(0, |p| p + 1)
}

/// The run of words starting at `start`: up to the next punctuation or break.
fn clause_end(tokens: &[Token], start: usize) -> usize {
    tokens[start..].iter().position(|t| !matches!(t, Token::Word(_))).map_or(tokens.len(), |p| start + p)
}

fn compatible(repair: Kind, found: Kind, before: Option<&str>) -> bool {
    repair == found || (repair == Kind::Time && found == Kind::Number && matches!(before, Some("at" | "by" | "around")))
}

/// The last phrase of a kind compatible with `repair` in `tokens[start..end]`.
fn nearest_phrase(tokens: &[Token], start: usize, end: usize, repair: Kind) -> Option<(usize, usize)> {
    let mut found = None;
    let mut i = start;
    while i < end {
        let run_end = clause_end(tokens, i).min(end);
        if run_end == i {
            i += 1;
            continue;
        }
        let run = words(&tokens[i..run_end]);
        let mut j = 0;
        while j < run.len() {
            match kinds::classify(&run[j..]) {
                Some((kind, len)) => {
                    let before = if j > 0 { Some(run[j - 1].as_str()) } else { None };
                    if compatible(repair, kind, before) {
                        found = Some((i + j, i + j + len));
                    }
                    j += len;
                }
                None => j += 1,
            }
        }
        i = run_end;
    }
    found
}

/// What a correction does: the text becomes `tokens[..from]`, then `tokens[repair]`,
/// then `tokens[kept]` (old words that stay), then `tokens[resume..]`.
struct Edit {
    from: usize,
    repair: Range<usize>,
    kept: Range<usize>,
    resume: usize,
}

fn plan(tokens: &[Token], at: usize, marker: Marker, marker_end: usize, language: Language) -> Option<Edit> {
    // Before the marker: its sentence, or the sentence before when it starts one.
    let mut pre_end = at;
    while pre_end > 0 && matches!(tokens[pre_end - 1], Token::Punct(_)) {
        pre_end -= 1;
    }
    if pre_end == 0 || !matches!(tokens[pre_end - 1], Token::Word(_)) {
        return None; // nothing before the marker to correct
    }
    let region = sentence_start(tokens, pre_end);
    let before_marker = word_at(tokens, at - 1).filter(|_| pre_end == at);
    match marker {
        Marker::Sorry if before_marker.as_deref().is_some_and(|w| APOLOGY.contains(&w)) => return None,
        Marker::Strong
            if word_at(tokens, at).as_deref() == Some("i")
                && before_marker.as_deref().is_some_and(|w| LITERAL_I_MEAN.contains(&w)) =>
        {
            return None;
        }
        _ => {}
    }

    // The repair: the clause after the marker, without a lead like "make it".
    let mut repair = marker_end;
    while matches!(tokens.get(repair), Some(Token::Punct(_))) {
        repair += 1;
    }
    let repair_end = clause_end(tokens, repair);
    let repair_words = words(&tokens[repair..repair_end]);
    let lead = REPAIR_LEADS
        .iter()
        .find(|l| {
            l.len() < repair_words.len()
                && l.iter().zip(&repair_words).all(|(a, b)| a == b)
                && kinds::classify(&repair_words[l.len()..]).is_some()
        })
        .map_or(0, |l| l.len());
    let repair_start = repair + lead;
    let repair_words = &repair_words[lead..];

    if let Some((kind, head_len)) = kinds::classify(repair_words)
        && let Some((p_start, p_end)) = nearest_phrase(tokens, region, pre_end, kind)
    {
        // Words between the replaced phrase and the marker.
        let between = words(&tokens[p_end..pre_end]);
        let restated = between.last().is_some_and(|w| repair_words.last() == Some(w));
        if between.is_empty() || restated {
            return Some(Edit { from: p_start, repair: repair_start..repair_end, kept: 0..0, resume: repair_end });
        }
        // "three people, I mean four" → "four people", "Monday ko aaunga, nahi nahi,
        // Tuesday ko" → "Tuesday ko aaunga": the repair is the phrase plus at most the
        // start of the words after the old one, and those words stay.
        let tail = &repair_words[head_len..];
        if between.starts_with(tail) {
            let head_end = repair_start + head_len;
            return Some(Edit {
                from: p_start,
                repair: repair_start..head_end,
                kept: p_end..pre_end,
                resume: repair_end,
            });
        }
    }

    if marker == Marker::Strong
        && language == Language::English
        && let Some(n) = noun_phrase(tokens, region, pre_end, repair_words)
    {
        return Some(Edit { from: pre_end - n, repair: repair_start..repair_end, kept: 0..0, resume: repair_end });
    }

    if marker == Marker::Scratch {
        let pause_before = pre_end < at;
        let pause_after = !matches!(tokens.get(marker_end), Some(Token::Word(_)));
        let object = word_at(tokens, marker_end).is_some_and(|w| SCRATCH_OBJECTS.contains(&w.as_str()));
        if (pause_before || pause_after) && !object && repair < tokens.len() {
            return Some(Edit { from: region, repair: 0..0, kept: 0..0, resume: repair });
        }
    }
    None
}

/// How many words before the marker a noun-phrase repair replaces, if it is one.
fn noun_phrase(tokens: &[Token], region: usize, pre_end: usize, repair: &[String]) -> Option<usize> {
    let n = repair.len();
    if !(1..=3).contains(&n) || pre_end < region + n {
        return None;
    }
    let old_tokens = &tokens[pre_end - n..pre_end];
    if !old_tokens.iter().all(|t| matches!(t, Token::Word(_))) {
        return None;
    }
    let old = words(old_tokens);
    let is_det = |w: &String| DETERMINERS.contains(&w.as_str());
    let both_det = is_det(&old[0]) && is_det(&repair[0]);
    let nounish = |ws: &[String]| {
        ws.iter().enumerate().all(|(i, w)| {
            // A lone date or number is not a noun phrase; "option one" is.
            (i == 0 && both_det)
                || (!NOT_NOUN.contains(&w.as_str()) && (n > 1 || kinds::classify(std::slice::from_ref(w)).is_none()))
        })
    };
    if !nounish(&old) || !nounish(repair) {
        return None;
    }
    if n > 1 && old[0] != repair[0] && old[n - 1] != repair[n - 1] && !both_det {
        return None;
    }
    // "talking to the boss, I mean Rahul": the old phrase is longer than the repair.
    let before = pre_end - n;
    if before > region && word_at(tokens, before - 1).is_some_and(|w| DETERMINERS.contains(&w.as_str())) {
        return None;
    }
    Some(n)
}

pub(super) fn apply(tokens: &mut Vec<Token>, language: Language) {
    let mut i = 0;
    while i < tokens.len() {
        let Some((marker, end)) = marker_at(tokens, i) else {
            i += 1;
            continue;
        };
        let Some(edit) = plan(tokens, i, marker, end, language) else {
            i = end;
            continue;
        };
        let mut next: Vec<Token> = Vec::with_capacity(tokens.len());
        next.extend_from_slice(&tokens[..edit.from]);
        next.extend_from_slice(&tokens[edit.repair]);
        next.extend_from_slice(&tokens[edit.kept]);
        next.extend_from_slice(&tokens[edit.resume..]);
        i = edit.from;
        *tokens = next;
    }
}
