//! WER / CER with normalization, plus latency percentiles.

use std::collections::HashMap;

/// Lowercase, strip punctuation, collapse whitespace. Keeps letters/digits of any
/// script (Devanagari included) and intra-word apostrophes ("don't").
pub fn normalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        let keep = c.is_alphanumeric()
            || is_devanagari_mark(c)
            || (c == '\''
                && i > 0
                && chars[i - 1].is_alphanumeric()
                && chars.get(i + 1).is_some_and(|n| n.is_alphanumeric()));
        if keep {
            out.extend(c.to_lowercase());
        } else {
            out.push(' ');
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Devanagari vowel signs / virama / nukta are not `alphanumeric` in Unicode but are
/// part of words.
fn is_devanagari_mark(c: char) -> bool {
    matches!(c, '\u{0900}'..='\u{0903}' | '\u{093A}'..='\u{094F}' | '\u{0951}'..='\u{0957}' | '\u{0962}'..='\u{0963}')
}

pub fn contains_devanagari(text: &str) -> bool {
    text.chars().any(|c| ('\u{0900}'..='\u{097F}').contains(&c))
}

/// Edit distance between token sequences.
fn levenshtein<T: PartialEq>(a: &[T], b: &[T]) -> usize {
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for (i, x) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, y) in b.iter().enumerate() {
            let sub = prev[j] + usize::from(x != y);
            cur[j + 1] = sub.min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// Errors and reference length, so corpus-level rates can be summed correctly.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ErrorCount {
    pub errors: usize,
    pub ref_len: usize,
}

impl ErrorCount {
    pub fn rate(&self) -> f64 {
        if self.ref_len == 0 {
            return if self.errors == 0 { 0.0 } else { 1.0 };
        }
        self.errors as f64 / self.ref_len as f64
    }
    pub fn add(&mut self, o: ErrorCount) {
        self.errors += o.errors;
        self.ref_len += o.ref_len;
    }
}

/// Normalized tokens with numbers canonicalized, so "4 pm", "four p.m." and "four PM"
/// all become `4 pm`, and "1,250" == "one thousand two hundred fifty" == "1250".
/// Applied identically to reference and hypothesis.
pub fn tokens(text: &str) -> Vec<String> {
    let words: Vec<String> =
        normalize(text).split(' ').filter(|w| !w.is_empty()).flat_map(split_digit_suffix).collect();
    let words = number_words_to_digits(&words);
    // "p m" / "a m" (from "p.m.") → "pm" / "am"
    let mut joined: Vec<String> = Vec::with_capacity(words.len());
    let mut i = 0;
    while i < words.len() {
        if (words[i] == "p" || words[i] == "a") && words.get(i + 1).is_some_and(|w| w == "m") {
            joined.push(format!("{}m", words[i]));
            i += 2;
        } else {
            joined.push(words[i].clone());
            i += 1;
        }
    }
    // Merge runs of digit tokens: "4 5 7 2" → "4572", "1 250" → "1250", "6 30" → "630".
    let mut out: Vec<String> = Vec::with_capacity(joined.len());
    for w in joined {
        let is_num = w.chars().all(|c| c.is_ascii_digit());
        match out.last_mut() {
            Some(prev) if is_num && prev.chars().all(|c| c.is_ascii_digit()) => prev.push_str(&w),
            _ => out.push(w),
        }
    }
    out
}

/// "4pm" → ["4", "pm"]; "06" → ["6"]; other words unchanged.
fn split_digit_suffix(w: &str) -> Vec<String> {
    let digits: String = w.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return vec![w.to_owned()];
    }
    let trimmed = digits.trim_start_matches('0');
    let num = if trimmed.is_empty() { "0".to_owned() } else { trimmed.to_owned() };
    let rest = &w[digits.len()..];
    match rest {
        "" => vec![num],
        "am" | "pm" => vec![num, rest.to_owned()],
        _ => vec![w.to_owned()], // "2nd", "4g": leave as-is
    }
}

fn small_number(w: &str) -> Option<(u64, NumKind)> {
    const UNITS: [&str; 20] = [
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
    ];
    const TENS: [&str; 8] = ["twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety"];
    if let Some(v) = UNITS.iter().position(|u| *u == w) {
        return Some((v as u64, if v < 10 { NumKind::Unit } else { NumKind::Teen }));
    }
    TENS.iter().position(|t| *t == w).map(|i| ((i as u64 + 2) * 10, NumKind::Tens))
}

#[derive(Clone, Copy, PartialEq)]
enum NumKind {
    Unit,
    Teen,
    Tens,
    Hundred,
    Scale,
}

/// "one thousand two hundred fifty" → "1250"; "four five seven two" → "4 5 7 2";
/// "six thirty" → "6 30". Non-number words pass through.
fn number_words_to_digits(words: &[String]) -> Vec<String> {
    let mut out = Vec::with_capacity(words.len());
    let mut total = 0u64;
    let mut small = 0u64;
    let mut last: Option<NumKind> = None;
    let flush = |out: &mut Vec<String>, total: &mut u64, small: &mut u64, last: &mut Option<NumKind>| {
        if last.is_some() {
            out.push((*total + *small).to_string());
        }
        *total = 0;
        *small = 0;
        *last = None;
    };
    for (i, w) in words.iter().enumerate() {
        if let Some((v, kind)) = small_number(w) {
            let starts_new = match (last, kind) {
                (Some(NumKind::Unit | NumKind::Teen), _) => true,
                (Some(NumKind::Tens), NumKind::Unit) => false,
                (Some(NumKind::Tens), _) => true,
                _ => false,
            };
            if starts_new {
                flush(&mut out, &mut total, &mut small, &mut last);
            }
            small += v;
            last = Some(kind);
        } else if w == "hundred" && last.is_some() {
            small = small.max(1) * 100;
            last = Some(NumKind::Hundred);
        } else if (w == "thousand" || w == "million") && last.is_some() {
            total += small.max(1) * if w == "thousand" { 1_000 } else { 1_000_000 };
            small = 0;
            last = Some(NumKind::Scale);
        } else if w == "and"
            && matches!(last, Some(NumKind::Hundred | NumKind::Scale))
            && words.get(i + 1).is_some_and(|n| small_number(n).is_some())
        {
            // "two hundred and fifty"
        } else {
            flush(&mut out, &mut total, &mut small, &mut last);
            out.push(w.clone());
        }
    }
    flush(&mut out, &mut total, &mut small, &mut last);
    out
}

/// Word errors after normalization. `aliases` maps accepted spelling variants to a
/// canonical form (Hinglish has no standard spelling: "nahi"/"nahin").
pub fn wer(reference: &str, hypothesis: &str, aliases: &HashMap<String, String>) -> ErrorCount {
    let canon = |w: String| aliases.get(&w).cloned().unwrap_or(w);
    let r: Vec<String> = tokens(reference).into_iter().map(canon).collect();
    let h: Vec<String> = tokens(hypothesis).into_iter().map(canon).collect();
    ErrorCount { errors: levenshtein(&r, &h), ref_len: r.len() }
}

/// Character errors after normalization, ignoring spaces.
pub fn cer(reference: &str, hypothesis: &str) -> ErrorCount {
    let r: Vec<char> = tokens(reference).concat().chars().collect();
    let h: Vec<char> = tokens(hypothesis).concat().chars().collect();
    ErrorCount { errors: levenshtein(&r, &h), ref_len: r.len() }
}

/// Nearest-rank percentile of `values` (milliseconds). `p` in `[0, 100]`.
pub fn percentile(values: &[f64], p: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut v = values.to_vec();
    v.sort_by(|a, b| a.total_cmp(b));
    let rank = ((p / 100.0) * v.len() as f64).ceil() as usize;
    Some(v[rank.clamp(1, v.len()) - 1])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_strips_punct_and_case() {
        assert_eq!(normalize("Hello, World!  It's  me."), "hello world it's me");
        assert_eq!(normalize("'quoted'"), "quoted");
        assert_eq!(normalize("मेरा phone नंबर है।"), "मेरा phone नंबर है");
    }

    #[test]
    fn wer_basic() {
        let none = HashMap::new();
        assert_eq!(wer("the cat sat", "the cat sat", &none).rate(), 0.0);
        let e = wer("the cat sat on the mat", "the cat sit on mat", &none);
        assert_eq!((e.errors, e.ref_len), (2, 6));
        assert_eq!(wer("", "", &none).rate(), 0.0);
        assert_eq!(wer("", "extra", &none).rate(), 1.0);
    }

    #[test]
    fn wer_aliases() {
        let a: HashMap<String, String> = [("nahin".to_string(), "nahi".to_string())].into();
        assert_eq!(wer("main nahi aaunga", "main nahin aaunga", &a).errors, 0);
    }

    #[test]
    fn numbers_are_canonical() {
        let none = HashMap::new();
        let pairs = [
            ("at 4 pm on Thursday", "at four p.m. on Thursday"),
            ("number is 4 5 7 2", "number is four five seven two"),
            ("comes to 1,250 dollars", "comes to one thousand two hundred fifty dollars"),
            ("lands at 6 30 in", "lands at six thirty in"),
            ("lands at 6:30 in", "lands at 6 30 in"),
            ("two hundred and five", "205"),
            ("twenty five", "25"),
            ("at 4 pm on", "at 4PM on"),
            ("lands at 6 30", "lands at 06:30"),
        ];
        for (a, b) in pairs {
            assert_eq!(wer(a, b, &none).errors, 0, "{a} | {b}");
        }
        // Different numbers still differ.
        assert_eq!(wer("at 4 pm", "at five pm", &none).errors, 1);
        assert_eq!(tokens("option two, it's cheaper"), ["option", "2", "it's", "cheaper"]);
    }

    #[test]
    fn cer_ignores_spaces() {
        assert_eq!(cer("ab cd", "abcd").errors, 0);
        assert_eq!(cer("abcd", "abed").errors, 1);
    }

    #[test]
    fn percentiles() {
        let v = [10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0, 100.0];
        assert_eq!(percentile(&v, 50.0), Some(50.0));
        assert_eq!(percentile(&v, 95.0), Some(100.0));
        assert_eq!(percentile(&[], 50.0), None);
    }

    #[test]
    fn detects_devanagari() {
        assert!(contains_devanagari("mera नाम"));
        assert!(!contains_devanagari("mera naam"));
    }
}
