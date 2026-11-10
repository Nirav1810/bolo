//! Numbers (ADR-0015): English number words become digits in the Bolo number style.
//!
//! - Cardinals become digits ("fifteen people" → "15 people"), except one through nine
//!   (and zero) in prose ("three people").
//! - Money, times, dates and percentages always use digits: "twelve fifty dollars" →
//!   "$1,250", "five thirty pm" → "5:30 pm", "march third" → "March 3", "twenty
//!   percent" → "20%". Numerals the engine wrote get the same treatment ("1,250
//!   dollars" → "$1,250", "at 6 30" → "at 6:30").
//! - Grouping is Indian ("₹1,25,000") when the dictation says lakh, crore or rupees, and
//!   Western ("125,000") otherwise. Outside money, four-digit numbers stay ungrouped
//!   ("1500", "2026", "4572"). A round lakh, crore, million or billion keeps its word
//!   ("2 crore", "$3 million").
//! - A long run of single digits spoken one by one is a code or a phone number: "four
//!   five seven two" → "4572".
//!
//! Only English number words are read. Hindi number words ("do sau pachaas") are left
//! for polish. When a run of number words doesn't read as one number and no money or time
//! context settles it ("twenty twenty six", "ten fifteen"), it stays verbatim.

use super::token::Token;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Grouping {
    Western,
    Indian,
}

/// A number word, by the part it plays in a spoken number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lex {
    /// zero … nine.
    Unit(u64),
    /// ten … nineteen.
    Teen(u64),
    /// twenty, thirty … ninety.
    Tens(u64),
    /// "twenty-five" written as one word.
    TensUnit(u64),
    Hundred,
    /// thousand, lakh, million, crore, billion.
    Scale(u64),
}

fn simple_lex(w: &str) -> Option<Lex> {
    use Lex::*;
    Some(match w {
        "zero" => Unit(0),
        "one" => Unit(1),
        "two" => Unit(2),
        "three" => Unit(3),
        "four" => Unit(4),
        "five" => Unit(5),
        "six" => Unit(6),
        "seven" => Unit(7),
        "eight" => Unit(8),
        "nine" => Unit(9),
        "ten" => Teen(10),
        "eleven" => Teen(11),
        "twelve" => Teen(12),
        "thirteen" => Teen(13),
        "fourteen" => Teen(14),
        "fifteen" => Teen(15),
        "sixteen" => Teen(16),
        "seventeen" => Teen(17),
        "eighteen" => Teen(18),
        "nineteen" => Teen(19),
        "twenty" => Tens(20),
        "thirty" => Tens(30),
        "forty" => Tens(40),
        "fifty" => Tens(50),
        "sixty" => Tens(60),
        "seventy" => Tens(70),
        "eighty" => Tens(80),
        "ninety" => Tens(90),
        "hundred" => Hundred,
        "thousand" => Scale(1_000),
        "lakh" | "lakhs" => Scale(100_000),
        "million" => Scale(1_000_000),
        "crore" | "crores" => Scale(10_000_000),
        "billion" => Scale(1_000_000_000),
        _ => return None,
    })
}

fn lex(w: Option<&String>) -> Option<Lex> {
    let w = w?;
    if let Some((tens, unit)) = w.split_once('-') {
        return match (simple_lex(tens), simple_lex(unit)) {
            (Some(Lex::Tens(t)), Some(Lex::Unit(u))) if u > 0 => Some(Lex::TensUnit(t + u)),
            _ => None,
        };
    }
    simple_lex(w)
}

/// Scales whose round values keep the word: "2 crore", "3 million".
fn keeps_word(scale: u64) -> bool {
    scale >= 100_000
}

const FULL_MONTHS: &[&str] = &[
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
];

/// Months that are also ordinary words ("march five miles", "you may"): they take a day
/// only as an ordinal ("march third").
const AMBIGUOUS_MONTHS: &[&str] = &["march", "may"];

/// Words before a bare "H M" that make it a time ("at six thirty").
const TIME_PREPOSITIONS: &[&str] = &["at", "by", "around", "till", "until", "from", "before", "after", "since"];

const AM_PM: &[&str] = &["am", "pm", "a.m", "p.m", "baje", "bje"];

const TIMES_OF_DAY: &[&str] = &["morning", "afternoon", "evening", "night"];

const CURRENCIES: &[(&str, &str)] = &[
    ("dollars", "$"),
    ("dollar", "$"),
    ("bucks", "$"),
    ("rupees", "₹"),
    ("rupee", "₹"),
    ("euros", "€"),
    ("euro", "€"),
];

fn ordinal_unit(w: &str) -> Option<u64> {
    Some(match w {
        "first" => 1,
        "second" => 2,
        "third" => 3,
        "fourth" => 4,
        "fifth" => 5,
        "sixth" => 6,
        "seventh" => 7,
        "eighth" => 8,
        "ninth" => 9,
        _ => return None,
    })
}

fn ordinal_word(w: &str) -> Option<u64> {
    if let Some(u) = ordinal_unit(w) {
        return Some(u);
    }
    Some(match w {
        "tenth" => 10,
        "eleventh" => 11,
        "twelfth" => 12,
        "thirteenth" => 13,
        "fourteenth" => 14,
        "fifteenth" => 15,
        "sixteenth" => 16,
        "seventeenth" => 17,
        "eighteenth" => 18,
        "nineteenth" => 19,
        "twentieth" => 20,
        "thirtieth" => 30,
        _ => return None,
    })
}

/// A day of the month spoken as an ordinal: "third", "twenty first", "twenty-first".
fn parse_ordinal_day(w: &[String], k: usize) -> Option<(u64, usize)> {
    let first = w.get(k)?;
    if let Some((tens, unit)) = first.split_once('-') {
        return match (simple_lex(tens), ordinal_unit(unit)) {
            (Some(Lex::Tens(t @ (20 | 30))), Some(u)) if t + u <= 31 => Some((t + u, 1)),
            _ => None,
        };
    }
    if let Some(d) = ordinal_word(first) {
        return (d <= 31).then_some((d, 1));
    }
    if let Some(Lex::Tens(t @ (20 | 30))) = simple_lex(first)
        && let Some(u) = w.get(k + 1).and_then(|x| ordinal_unit(x))
        && t + u <= 31
    {
        return Some((t + u, 2));
    }
    None
}

fn ordinal_suffix(n: u64) -> &'static str {
    match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    }
}

/// 1 … 99 as a number's last part: "five", "fifteen", "fifty", "fifty five".
fn parse_tail(w: &[String], k: usize) -> Option<(u64, usize)> {
    match lex(w.get(k))? {
        Lex::Unit(u) if u > 0 => Some((u, k + 1)),
        Lex::Teen(t) | Lex::TensUnit(t) => Some((t, k + 1)),
        Lex::Tens(t) => match lex(w.get(k + 1)) {
            Some(Lex::Unit(u)) if u > 0 => Some((t + u, k + 2)),
            _ => Some((t, k + 1)),
        },
        _ => None,
    }
}

/// A group under a scale word: "five", "twenty five", "two hundred and five", "twelve
/// hundred fifty".
fn parse_group(w: &[String], k: usize) -> Option<(u64, usize)> {
    let (mut v, mut end) = match lex(w.get(k))? {
        Lex::Unit(u) => (u, k + 1),
        _ => parse_tail(w, k)?,
    };
    if v > 0 && lex(w.get(end)) == Some(Lex::Hundred) {
        v *= 100;
        end += 1;
        let t = if w.get(end).is_some_and(|x| x == "and") { end + 1 } else { end };
        if let Some((tail, t_end)) = parse_tail(w, t) {
            v += tail;
            end = t_end;
        }
    }
    Some((v, end))
}

/// A spoken cardinal starting at `w[k]`.
struct Cardinal {
    value: u64,
    end: usize,
    /// The scale word of a round value that keeps it ("two crore"): its index.
    round_scale: Option<usize>,
}

fn parse_cardinal(w: &[String], k: usize) -> Option<Cardinal> {
    let mut total = 0u64;
    let mut last_scale = u64::MAX;
    let mut end = k;
    let mut scales = 0;
    let mut round_scale = None;
    loop {
        let start = if total > 0 && w.get(end).is_some_and(|x| x == "and") { end + 1 } else { end };
        let Some((group, group_end)) = parse_group(w, start) else { break };
        match lex(w.get(group_end)) {
            Some(Lex::Scale(s)) if s < last_scale && group > 0 => {
                total = total.checked_add(group.checked_mul(s)?)?;
                last_scale = s;
                scales += 1;
                round_scale = (scales == 1 && keeps_word(s)).then_some(group_end);
                end = group_end + 1;
            }
            Some(Lex::Scale(_)) => break,
            _ => {
                if total == 0 || group < last_scale {
                    total += group;
                    end = group_end;
                    if group > 0 {
                        round_scale = None;
                    }
                }
                break;
            }
        }
    }
    if end == k {
        return None;
    }
    if scales > 1 {
        round_scale = None;
    }
    Some(Cardinal { value: total, end, round_scale })
}

/// A numeral the engine wrote: "1,250", "125000", "3.5". No currency symbol or unit.
fn parse_numeral(w: &str) -> Option<(u64, Option<&str>)> {
    let (int, frac) = match w.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (w, None),
    };
    let digits_ok = int.starts_with(|c: char| c.is_ascii_digit())
        && int.chars().all(|c| c.is_ascii_digit() || c == ',')
        && frac.is_none_or(|f| !f.is_empty() && f.chars().all(|c| c.is_ascii_digit()));
    if !digits_ok {
        return None;
    }
    let value = int.chars().filter(char::is_ascii_digit).collect::<String>().parse().ok()?;
    Some((value, frac))
}

/// Digits with thousands separators. `min` is the smallest value that gets them.
fn group(n: u64, grouping: Grouping, min: u64) -> String {
    let s = n.to_string();
    if n < min || s.len() <= 3 {
        return s;
    }
    let (head, last3) = s.split_at(s.len() - 3);
    let step = match grouping {
        Grouping::Western => 3,
        Grouping::Indian => 2,
    };
    let mut parts = Vec::new();
    let mut rest = head;
    while rest.len() > step {
        let (h, t) = rest.split_at(rest.len() - step);
        parts.push(t);
        rest = h;
    }
    parts.push(rest);
    parts.reverse();
    format!("{},{last3}", parts.join(","))
}

/// A number ready to write: its digits, and the scale word it keeps, if any.
struct Number {
    int: u64,
    frac: Option<String>,
    scale_word: Option<String>,
    /// One word, zero through nine: prose keeps it as a word.
    small_word: bool,
    end: usize,
}

impl Number {
    fn render(&self, grouping: Grouping, min_group: u64) -> String {
        let mut s = group(self.int, grouping, min_group);
        if let Some(f) = &self.frac {
            s.push('.');
            s.push_str(f);
        }
        if let Some(scale) = &self.scale_word {
            s.push(' ');
            s.push_str(scale);
        }
        s
    }
}

/// Digits spoken after "point": "point five", "point two five".
fn parse_fraction(w: &[String], k: usize) -> Option<(String, usize)> {
    if w.get(k).is_none_or(|x| x != "point") {
        return None;
    }
    let mut digits = String::new();
    let mut end = k + 1;
    while let Some(Lex::Unit(u)) = lex(w.get(end)) {
        digits.push(char::from(b'0' + u as u8));
        end += 1;
    }
    (!digits.is_empty()).then_some((digits, end))
}

/// A spoken number with its decimals and kept scale word, or a numeral.
fn parse_number(w: &[String], orig: &[String], k: usize) -> Option<Number> {
    if let Some((int, frac)) = parse_numeral(&w[k]) {
        return Some(Number { int, frac: frac.map(str::to_owned), scale_word: None, small_word: false, end: k + 1 });
    }
    let c = parse_cardinal(w, k)?;
    let mut n = Number {
        int: c.value,
        frac: None,
        scale_word: c.round_scale.map(|i| orig[i].clone()),
        small_word: c.end == k + 1 && c.value <= 9,
        end: c.end,
    };
    if c.round_scale.is_some() {
        n.int = c.value / scale_of(&w[c.end - 1]);
        return Some(n);
    }
    if let Some((frac, end)) = parse_fraction(w, c.end) {
        n.frac = Some(frac);
        n.small_word = false;
        n.end = end;
        // "two point five crore"; "two point five thousand" is unsure.
        match lex(w.get(end)) {
            Some(Lex::Scale(s)) if keeps_word(s) => {
                n.scale_word = Some(orig[end].clone());
                n.end = end + 1;
            }
            Some(Lex::Scale(_) | Lex::Hundred) => return None,
            _ => {}
        }
    }
    Some(n)
}

fn scale_of(w: &str) -> u64 {
    match simple_lex(w) {
        Some(Lex::Scale(s)) => s,
        _ => 1,
    }
}

fn is_number_word(w: Option<&String>) -> bool {
    lex(w).is_some()
}

/// Whether a number follows at `k`: a number word, or anything written in digits
/// ("4", "$4", "6:30", "20%"). Digits count because this pass writes them: the answer
/// must be the same on the pass's own output, or it isn't idempotent.
fn number_follows(w: &[String], k: usize) -> bool {
    is_number_word(w.get(k))
        || w.get(k)
            .is_some_and(|x| x.trim_start_matches(['$', '₹', '€', '£']).starts_with(|c: char| c.is_ascii_digit()))
}

/// A single digit spoken or written alone: "four", "4".
fn single_digit(w: &str) -> Option<char> {
    match simple_lex(w) {
        Some(Lex::Unit(u)) => Some(char::from(b'0' + u as u8)),
        _ if w.len() == 1 && w.as_bytes()[0].is_ascii_digit() => w.chars().next(),
        _ => None,
    }
}

/// An hour, 1 … 12: "six", "twelve", "6".
fn hour(w: &str) -> Option<u64> {
    let h = match simple_lex(w) {
        Some(Lex::Unit(u) | Lex::Teen(u)) => u,
        _ if w.len() <= 2 && w.chars().all(|c| c.is_ascii_digit()) => w.parse().ok()?,
        _ => return None,
    };
    (1..=12).contains(&h).then_some(h)
}

/// Minutes after an hour: "thirty", "forty five", "oh five", "30".
fn minutes(w: &[String], k: usize) -> Option<(u64, usize)> {
    let first = w.get(k)?;
    if first.len() == 2 && first.chars().all(|c| c.is_ascii_digit()) {
        let m: u64 = first.parse().ok()?;
        return (m <= 59).then_some((m, k + 1));
    }
    if first == "oh"
        && let Some(Lex::Unit(u)) = lex(w.get(k + 1))
        && u > 0
    {
        return Some((u, k + 2));
    }
    match parse_tail(w, k)? {
        (m, end) if (10..=59).contains(&m) => Some((m, end)),
        _ => None,
    }
}

/// Whether the words at `k` mark a time: "pm", "in the evening", "at night", "tonight".
fn time_follows(w: &[String], k: usize) -> bool {
    let at = |i: usize| w.get(k + i).map(String::as_str);
    match at(0) {
        Some(x) if AM_PM.contains(&x) || x == "tonight" => true,
        Some("in") => at(1) == Some("the") && at(2).is_some_and(|x| TIMES_OF_DAY.contains(&x)),
        Some("at") => at(1) == Some("night"),
        _ => false,
    }
}

fn currency(w: Option<&String>) -> Option<&'static str> {
    let w = w?;
    CURRENCIES.iter().find(|(word, _)| w == word).map(|(_, sym)| *sym)
}

/// Whether a money, percent or clock unit follows at `k`: the number before it isn't a day.
fn unit_follows(w: &[String], k: usize) -> bool {
    currency(w.get(k)).is_some()
        || w.get(k).is_some_and(|x| x == "percent" || x == "per" || x == "o'clock" || x == "cents")
        || time_follows(w, k)
}

fn capitalized(w: &str) -> String {
    let mut c = w.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

/// What the words at `w[i..]` become, and how many words they replace.
type Conversion = (Vec<String>, usize);

/// "March 3", "March 21", "3rd of March", "5th June".
fn date_at(w: &[String], orig: &[String], i: usize) -> Option<Conversion> {
    let word = w[i].as_str();
    if FULL_MONTHS.contains(&word) {
        let (day, end) = match parse_ordinal_day(w, i + 1) {
            Some((d, len)) => (d, i + 1 + len),
            None if !AMBIGUOUS_MONTHS.contains(&word) => match parse_tail(w, i + 1)? {
                (d, end) if d <= 31 => (d, end),
                _ => return None,
            },
            None => return None,
        };
        if number_follows(w, end) || unit_follows(w, end) {
            return None; // "june twenty twenty six", "june one dollars": unsure
        }
        return Some((vec![capitalized(&orig[i]), day.to_string()], end - i));
    }
    let (day, len) = parse_ordinal_day(w, i)?;
    let month_at = if w.get(i + len).is_some_and(|x| x == "of") { i + len + 1 } else { i + len };
    if !w.get(month_at).is_some_and(|m| FULL_MONTHS.contains(&m.as_str())) {
        return None;
    }
    if parse_ordinal_day(w, month_at + 1).is_some() || number_follows(w, month_at + 1) {
        return None; // "third june one": the month has a day of its own
    }
    let mut out = vec![format!("{day}{}", ordinal_suffix(day))];
    out.extend(orig[i + len..month_at].iter().cloned());
    out.push(capitalized(&orig[month_at]));
    Some((out, month_at + 1 - i))
}

/// "four five seven two" → "4572": at least four single digits in a row.
fn digit_run_at(w: &[String], i: usize) -> Option<Conversion> {
    let digits: String = w[i..].iter().map_while(|x| single_digit(x)).collect();
    if digits.len() < 4 || number_follows(w, i + digits.len()) {
        return None;
    }
    let len = digits.len();
    Some((vec![digits], len))
}

/// "six thirty pm", "at six thirty", "at 6 30" → "6:30".
fn clock_at(w: &[String], i: usize) -> Option<Conversion> {
    let h = hour(&w[i])?;
    let (m, end) = minutes(w, i + 1)?;
    let after_preposition = i > 0 && TIME_PREPOSITIONS.contains(&w[i - 1].as_str());
    if number_follows(w, end) || currency(w.get(end)).is_some() || !(after_preposition || time_follows(w, end)) {
        return None;
    }
    Some((vec![format!("{h}:{m:02}")], end - i))
}

/// "twelve fifty dollars" → "$1,250": two numbers under 100 spoken as hundreds.
fn hundreds_pair(w: &[String], i: usize) -> Option<(u64, usize)> {
    let (head, head_end) = parse_tail(w, i)?;
    let (tail, end) = parse_tail(w, head_end)?;
    ((10..=99).contains(&head) && (10..=99).contains(&tail)).then_some((head * 100 + tail, end))
}

/// "and fifty cents", "fifty cents", after a dollar amount.
fn cents(w: &[String], k: usize) -> Option<(u64, usize)> {
    let start = if w.get(k).is_some_and(|x| x == "and") { k + 1 } else { k };
    let (c, end) = parse_tail(w, start)?;
    w.get(end).is_some_and(|x| x == "cents" || x == "cent").then_some((c, end + 1))
}

fn money_at(w: &[String], orig: &[String], i: usize, grouping: Grouping) -> Option<Conversion> {
    let (number, end) = match hundreds_pair(w, i) {
        Some((v, end)) if currency(w.get(end)).is_some() => {
            (Number { int: v, frac: None, scale_word: None, small_word: false, end }, end)
        }
        _ => {
            let n = parse_number(w, orig, i)?;
            let end = n.end;
            (n, end)
        }
    };
    let symbol = currency(w.get(end))?;
    let grouping = if symbol == "₹" { Grouping::Indian } else { grouping };
    let mut text = format!("{symbol}{}", number.render(grouping, 1_000));
    let mut len = end + 1 - i;
    if symbol == "$"
        && number.frac.is_none()
        && number.scale_word.is_none()
        && let Some((c, c_end)) = cents(w, end + 1)
    {
        text = format!("${}.{c:02}", group(number.int, grouping, 1_000));
        len = c_end - i;
    }
    Some((vec![text], len))
}

fn convert_at(w: &[String], orig: &[String], i: usize, grouping: Grouping) -> Option<Conversion> {
    if let Some(c) = date_at(w, orig, i) {
        return Some(c);
    }
    if let Some(c) = digit_run_at(w, i) {
        return Some(c);
    }
    if let Some(c) = money_at(w, orig, i, grouping) {
        return Some(c);
    }
    if let Some(c) = clock_at(w, i) {
        return Some(c);
    }
    let n = parse_number(w, orig, i)?;
    let numeral = parse_numeral(&w[i]).is_some();
    let next = w.get(n.end).map(String::as_str);
    if next == Some("percent") || (next == Some("per") && w.get(n.end + 1).is_some_and(|x| x == "cent")) {
        let len = if next == Some("per") { n.end + 2 - i } else { n.end + 1 - i };
        let digits = if numeral { orig[i].clone() } else { n.render(grouping, 10_000) };
        return Some((vec![format!("{digits}%")], len));
    }
    if numeral {
        return None; // a numeral the engine wrote is already in style
    }
    if number_follows(w, n.end) {
        return None; // two numbers in a row that don't read as one: unsure
    }
    // "five pm", "five o'clock", "five in the evening"
    let is_time = hour(&w[i]).is_some() && (time_follows(w, n.end) || next == Some("o'clock"));
    if n.small_word && !is_time {
        return None;
    }
    Some((vec![n.render(grouping, 10_000)], n.end - i))
}

/// Whether the dictation speaks in lakhs, crores or rupees.
fn grouping_of(tokens: &[Token]) -> Grouping {
    let indian = tokens.iter().any(|t| match t {
        Token::Word(w) => {
            w.starts_with('₹') || {
                let w = w.to_lowercase();
                matches!(w.as_str(), "lakh" | "lakhs" | "crore" | "crores" | "rupees" | "rupee")
            }
        }
        _ => false,
    });
    if indian { Grouping::Indian } else { Grouping::Western }
}

pub(super) fn apply(tokens: &mut Vec<Token>) {
    let grouping = grouping_of(tokens);
    let src = std::mem::take(tokens);
    tokens.reserve(src.len());
    let mut i = 0;
    while i < src.len() {
        let run_len = src[i..].iter().take_while(|t| matches!(t, Token::Word(_))).count();
        if run_len == 0 {
            tokens.push(src[i].clone());
            i += 1;
            continue;
        }
        let orig: Vec<String> = src[i..i + run_len]
            .iter()
            .map(|t| match t {
                Token::Word(w) => w.clone(),
                _ => String::new(),
            })
            .collect();
        convert_run(&orig, grouping, tokens);
        i += run_len;
    }
}

/// How many words from `k` are number words, or "and" / "point" between them.
fn number_stretch(w: &[String], k: usize) -> usize {
    let mut end = k;
    while is_number_word(w.get(end))
        || (end > k && w.get(end).is_some_and(|x| x == "and" || x == "point") && is_number_word(w.get(end + 1)))
    {
        end += 1;
    }
    end - k
}

/// Convert the numbers in a run of words with no punctuation between them.
fn convert_run(orig: &[String], grouping: Grouping, out: &mut Vec<Token>) {
    let w: Vec<String> = orig.iter().map(|x| x.to_lowercase()).collect();
    let mut k = 0;
    while k < w.len() {
        if let Some((words, len)) = convert_at(&w, orig, k, grouping) {
            out.extend(words.into_iter().map(Token::Word));
            k += len;
            continue;
        }
        // Number words that don't convert stay verbatim as a whole run, so that the
        // tail of "twenty twenty six" isn't read on its own.
        let verbatim = if is_number_word(w.get(k)) { number_stretch(&w, k) } else { 1 };
        out.extend(orig[k..k + verbatim].iter().cloned().map(Token::Word));
        k += verbatim;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grouping() {
        assert_eq!(group(125_000, Grouping::Western, 1_000), "125,000");
        assert_eq!(group(125_000, Grouping::Indian, 1_000), "1,25,000");
        assert_eq!(group(12_345_678, Grouping::Indian, 1_000), "1,23,45,678");
        assert_eq!(group(1_234_567, Grouping::Western, 1_000), "1,234,567");
        assert_eq!(group(4_572, Grouping::Western, 10_000), "4572");
        assert_eq!(group(999, Grouping::Western, 0), "999");
    }

    fn words(s: &str) -> Vec<String> {
        s.split(' ').map(str::to_owned).collect()
    }

    #[test]
    fn cardinals() {
        let v = |s: &str| parse_cardinal(&words(s), 0).map(|c| (c.value, c.end));
        assert_eq!(v("twenty five"), Some((25, 2)));
        assert_eq!(v("one hundred and five"), Some((105, 4)));
        assert_eq!(v("twelve hundred fifty"), Some((1250, 3)));
        assert_eq!(v("one lakh twenty five thousand"), Some((125_000, 5)));
        assert_eq!(v("five thousand two thousand"), Some((5000, 2)));
        assert_eq!(v("twelve fifty"), Some((12, 1)));
        assert_eq!(v("two and three"), Some((2, 1)));
        assert_eq!(v("lakh"), None);
    }
}
