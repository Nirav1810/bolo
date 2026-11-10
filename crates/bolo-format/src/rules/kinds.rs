//! Phrase kinds the self-correction pass matches on: dates, times and numbers (ADR-0015).
//!
//! [`classify`] looks at the start of a run of words and says which kind of phrase starts
//! there and how many words it spans. It reads lowercase words. A word that fits no kind
//! gives `None`, and the caller treats it as part of an ordinary phrase.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    /// "Tuesday", "next week", "kal shaam", "5th June".
    Date,
    /// "5 pm", "5:30", "noon", "evening".
    Time,
    /// "1,250", "twelve fifty", "20%", "3 dollars".
    Number,
}

const WEEKDAYS: &[&str] = &[
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
    "sunday",
    "mondays",
    "tuesdays",
    "wednesdays",
    "thursdays",
    "fridays",
    "saturdays",
    "sundays",
];

const MONTHS: &[&str] = &[
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
    "jan",
    "feb",
    "mar",
    "apr",
    "jun",
    "jul",
    "aug",
    "sep",
    "sept",
    "oct",
    "nov",
    "dec",
];

/// Days named relative to today, in English and Hinglish.
const RELATIVE_DAYS: &[&str] =
    &["today", "tomorrow", "yesterday", "tonight", "kal", "aaj", "parson", "parso", "parsoon"];

/// Spans a modifier makes into a date: "next week", "this weekend".
const SPANS: &[&str] = &["week", "weekend", "month", "year"];

const DATE_MODIFIERS: &[&str] = &["next", "this", "last", "coming", "agle", "agla", "pichle"];

const TIMES_OF_DAY: &[&str] =
    &["morning", "afternoon", "evening", "night", "noon", "midnight", "shaam", "subah", "raat", "dopahar"];

const TIME_SUFFIXES: &[&str] = &["am", "pm", "a.m", "p.m", "o'clock", "baje", "bje"];

const NUMBER_WORDS: &[&str] = &[
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
    "thirty",
    "forty",
    "fifty",
    "sixty",
    "seventy",
    "eighty",
    "ninety",
    "hundred",
    "thousand",
    "lakh",
    "lakhs",
    "crore",
    "crores",
    "million",
    "billion",
];

const ORDINAL_WORDS: &[&str] = &[
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
    "fifteenth",
    "twentieth",
    "thirtieth",
];

/// Units that belong to the number before them.
const NUMBER_UNITS: &[&str] = &["dollars", "dollar", "bucks", "rupees", "rupee", "percent", "%", "k"];

/// A spoken or written number: "twelve", "1,250", "$40", "₹500", "20%", "3.5".
pub(super) fn is_number(word: &str) -> bool {
    if NUMBER_WORDS.contains(&word) {
        return true;
    }
    let digits = word.trim_start_matches(['$', '₹', '€', '£']).trim_end_matches(['%', 'k']);
    digits.starts_with(|c: char| c.is_ascii_digit())
        && digits.chars().all(|c| c.is_ascii_digit() || matches!(c, ',' | '.'))
}

/// "5th", "21st", "fifth".
fn is_ordinal(word: &str) -> bool {
    if ORDINAL_WORDS.contains(&word) {
        return true;
    }
    let digits = word.trim_end_matches(char::is_alphabetic);
    !digits.is_empty()
        && digits.chars().all(|c| c.is_ascii_digit())
        && matches!(&word[digits.len()..], "st" | "nd" | "rd" | "th")
}

/// "5:30", "10:00".
fn is_clock(word: &str) -> bool {
    let Some((h, m)) = word.split_once(':') else { return false };
    (1..=2).contains(&h.len())
        && m.len() == 2
        && h.chars().all(|c| c.is_ascii_digit())
        && m.chars().all(|c| c.is_ascii_digit())
}

/// The kind of phrase that starts at `words[0]`, and how many words it spans.
pub(super) fn classify<S: AsRef<str>>(words: &[S]) -> Option<(Kind, usize)> {
    let w = |i: usize| words.get(i).map(AsRef::as_ref);
    let first = w(0)?;
    let in_list = |i: usize, list: &[&str]| w(i).is_some_and(|x| list.contains(&x));
    // A time of day may follow a date: "kal shaam", "Tuesday morning".
    let with_time_of_day = |len: usize| if in_list(len, TIMES_OF_DAY) { len + 1 } else { len };

    if DATE_MODIFIERS.contains(&first) && (in_list(1, WEEKDAYS) || in_list(1, SPANS)) {
        return Some((Kind::Date, with_time_of_day(2)));
    }
    if WEEKDAYS.contains(&first) || RELATIVE_DAYS.contains(&first) {
        return Some((Kind::Date, with_time_of_day(1)));
    }
    if MONTHS.contains(&first) && first != "may" && first != "mar" {
        let len = if w(1).is_some_and(|x| is_ordinal(x) || is_number(x)) { 2 } else { 1 };
        return Some((Kind::Date, len));
    }
    if is_ordinal(first) || is_number(first) {
        // "5th June", "5th of June".
        if in_list(1, MONTHS) {
            return Some((Kind::Date, 2));
        }
        if w(1) == Some("of") && in_list(2, MONTHS) {
            return Some((Kind::Date, 3));
        }
    }
    if is_clock(first) {
        let len = if in_list(1, TIME_SUFFIXES) { 2 } else { 1 };
        return Some((Kind::Time, len));
    }
    if TIMES_OF_DAY.contains(&first) {
        return Some((Kind::Time, 1));
    }
    if is_number(first) {
        let mut len = 1;
        while w(len).is_some_and(is_number) {
            len += 1;
        }
        if in_list(len, TIME_SUFFIXES) {
            return Some((Kind::Time, len + 1));
        }
        if in_list(len, NUMBER_UNITS) {
            len += 1;
        }
        return Some((Kind::Number, len));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind(text: &str) -> Option<(Kind, usize)> {
        classify(&text.split(' ').collect::<Vec<_>>())
    }

    #[test]
    fn dates() {
        assert_eq!(kind("tuesday after lunch"), Some((Kind::Date, 1)));
        assert_eq!(kind("next friday"), Some((Kind::Date, 2)));
        assert_eq!(kind("kal shaam ko"), Some((Kind::Date, 2)));
        assert_eq!(kind("june 5th"), Some((Kind::Date, 2)));
        assert_eq!(kind("5th of june"), Some((Kind::Date, 3)));
        assert_eq!(kind("may i"), None);
    }

    #[test]
    fn times_and_numbers() {
        assert_eq!(kind("5 pm"), Some((Kind::Time, 2)));
        assert_eq!(kind("5:30 in"), Some((Kind::Time, 1)));
        assert_eq!(kind("evening"), Some((Kind::Time, 1)));
        assert_eq!(kind("twelve fifty dollars"), Some((Kind::Number, 3)));
        assert_eq!(kind("$1,250"), Some((Kind::Number, 1)));
        assert_eq!(kind("20%"), Some((Kind::Number, 1)));
        assert_eq!(kind("three people"), Some((Kind::Number, 1)));
        assert_eq!(kind("people"), None);
    }
}
