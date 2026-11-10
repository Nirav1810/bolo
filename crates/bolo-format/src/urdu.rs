//! Urdu script → Devanagari, so Hindi that an engine wrote in Urdu script can go
//! through the same Devanagari → Roman romanizer and loanword lexicon
//! ([`crate::loanwords::to_roman_hinglish`]).
//!
//! Deepgram `multi` sometimes writes a Hindi or English word in Urdu script ("تیم" for
//! "team"). Urdu leaves most short vowels unwritten and uses one letter for several
//! vowels (و is v, o or oo; ی is y, i or ee; ے is e or ai), so the map is a best guess:
//! - the most frequent function words come from a whole-word table, so they match the
//!   spelling the Devanagari path gives (ہے → है → "hai", کیا → क्या → "kya");
//! - every other word is mapped letter by letter: ا, و, ی and ے become vowel signs after
//!   a consonant and independent vowels or semivowels elsewhere; ھ aspirates the
//!   consonant before it (کھ → ख); ں nasalizes; a final ہ after a consonant is "a".
//!
//! Every Arabic-script character comes out as Devanagari, ASCII punctuation or a
//! digit, or is dropped. None survive, so Roman-script output never mixes scripts.

/// True if `text` contains any Arabic-script character (Urdu letters, marks, digits or
/// punctuation, including presentation forms).
pub fn has_urdu(text: &str) -> bool {
    text.chars().any(is_arabic_script)
}

fn is_arabic_script(c: char) -> bool {
    matches!(c,
        '\u{0600}'..='\u{06FF}'
        | '\u{0750}'..='\u{077F}'
        | '\u{0870}'..='\u{08FF}'
        | '\u{FB50}'..='\u{FDFF}'
        | '\u{FE70}'..='\u{FEFF}')
        // U+FEFF is the byte-order mark, not Arabic.
        && c != '\u{FEFF}'
}

/// Map every Urdu-script word in `text` to Devanagari; everything else passes through.
/// Urdu punctuation becomes ASCII and Arabic-Indic digits become ASCII digits.
pub fn urdu_to_devanagari(text: &str) -> String {
    if !has_urdu(text) {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len() * 2);
    let mut word: Vec<char> = Vec::new();
    for c in text.chars() {
        if is_word_char(c) {
            word.push(normalize(c));
            continue;
        }
        flush(&mut word, &mut out);
        match c {
            '،' => out.push(','),
            '۔' => out.push('.'),
            '؟' => out.push('?'),
            '؛' => out.push(';'),
            '٪' => out.push('%'),
            '\u{0660}'..='\u{0669}' => out.push(char::from(b'0' + (c as u32 - 0x0660) as u8)),
            '\u{06F0}'..='\u{06F9}' => out.push(char::from(b'0' + (c as u32 - 0x06F0) as u8)),
            c if is_arabic_script(c) => {} // tatweel, presentation forms, other signs
            c => out.push(c),
        }
    }
    flush(&mut word, &mut out);
    out
}

/// Letters and combining marks that belong inside a word.
fn is_word_char(c: char) -> bool {
    matches!(c, '\u{0621}'..='\u{063A}' | '\u{0641}'..='\u{0655}' | '\u{0670}'..='\u{06D3}') && !matches!(c, '۔' | '٪')
}

/// Fold Arabic letter variants onto the Urdu ones.
fn normalize(c: char) -> char {
    match c {
        'ك' => 'ک',
        'ي' | 'ى' | 'ې' => 'ی',
        'ه' | 'ە' => 'ہ',
        'ة' | 'ۃ' => 'ہ',
        'أ' | 'إ' | 'ٱ' => 'ا',
        'ؤ' => 'و',
        'ۓ' => 'ے',
        c => c,
    }
}

fn flush(word: &mut Vec<char>, out: &mut String) {
    if word.is_empty() {
        return;
    }
    let w: String = word.iter().collect();
    match common_word(&w) {
        Some(d) => out.push_str(d),
        None => word_to_devanagari(word, out),
    }
    word.clear();
}

/// Frequent Hindi words whose Urdu spelling the letter map would get wrong (unwritten
/// short vowels, ے for "ai"), written the way the Devanagari path expects them.
fn common_word(w: &str) -> Option<&'static str> {
    Some(match w {
        "ہے" => "है",
        "ہیں" => "हैं",
        "ہوں" => "हूँ",
        "ہو" => "हो",
        "ہوا" => "हुआ",
        "ہوئی" => "हुई",
        "ہوئے" => "हुए",
        "تھا" => "था",
        "تھی" => "थी",
        "تھے" => "थे",
        "میں" => "में",
        "مَیں" => "मैं",
        "کیا" => "क्या",
        "کیوں" => "क्यों",
        "کیسے" => "कैसे",
        "کیسا" => "कैसा",
        "کہ" => "कि",
        "یہ" => "ये",
        "وہ" => "वो",
        "اور" => "और",
        "نہیں" => "नहीं",
        "نہ" => "ना",
        "کا" => "का",
        "کی" => "की",
        "کے" => "के",
        "کو" => "को",
        "سے" => "से",
        "پر" => "पर",
        "پے" => "पे",
        "تک" => "तक",
        "نے" => "ने",
        "لیے" | "لئے" => "लिए",
        "تو" => "तो",
        "بھی" => "भी",
        "ہی" => "ही",
        "ہم" => "हम",
        "تم" => "तुम",
        "آپ" => "आप",
        "مجھے" => "मुझे",
        "مجھ" => "मुझ",
        "تجھے" => "तुझे",
        "اس" => "इस",
        "اسے" => "इसे",
        "ان" => "इन",
        "کر" => "कर",
        "گیا" => "गया",
        "گئی" => "गई",
        "گئے" => "गए",
        "رہا" => "रहा",
        "رہی" => "रही",
        "رہے" => "रहे",
        "اب" => "अब",
        "جب" => "जब",
        "کب" => "कब",
        "سب" => "सब",
        "کچھ" => "कुछ",
        "کوئی" => "कोई",
        "ایک" => "एक",
        "ابھی" => "अभी",
        "پھر" => "फिर",
        "ٹھیک" => "ठीक",
        "ہاں" => "हाँ",
        "بہت" => "बहुत",
        "لیکن" => "लेकिन",
        "اگر" => "अगर",
        "کل" => "कल",
        "آج" => "आज",
        "دن" => "दिन",
        "کام" => "काम",
        "بات" => "बात",
        "والا" => "वाला",
        "والی" => "वाली",
        "والے" => "वाले",
        "دینا" => "देना",
        "لینا" => "लेना",
        "بھیج" => "भेज",
        "بھیجو" => "भेजो",
        "دیکھ" => "देख",
        "دیکھو" => "देखो",
        "دیکھا" => "देखा",
        "پہلے" => "पहले",
        "میرا" => "मेरा",
        "میری" => "मेरी",
        "میرے" => "मेरे",
        "تیرا" => "तेरा",
        "دیر" => "देर",
        "رنگ" => "रंग",
        "شیئر" => "शेयर",
        _ => return None,
    })
}

/// Consonant letters (two-letter aspirates are handled by the caller via ھ).
fn consonant(c: char) -> Option<&'static str> {
    Some(match c {
        'ب' => "ब",
        'پ' => "प",
        'ت' | 'ط' => "त",
        'ٹ' => "ट",
        'ث' | 'س' | 'ص' => "स",
        'ج' => "ज",
        'چ' => "च",
        'ح' | 'ہ' => "ह",
        'خ' => "ख़",
        'د' => "द",
        'ڈ' => "ड",
        'ذ' | 'ز' | 'ض' | 'ظ' | 'ژ' => "ज़",
        'ر' => "र",
        'ڑ' => "ड़",
        'ش' => "श",
        'غ' => "ग़",
        'ف' => "फ़",
        'ق' => "क़",
        'ک' => "क",
        'گ' => "ग",
        'ل' => "ल",
        'م' => "म",
        'ن' => "न",
        _ => return None,
    })
}

/// Aspirated form of a consonant followed by ھ (do-chashmi he).
fn aspirated(c: char) -> Option<&'static str> {
    Some(match c {
        'ب' => "भ",
        'پ' => "फ",
        'ت' => "थ",
        'ٹ' => "ठ",
        'ج' => "झ",
        'چ' => "छ",
        'د' => "ध",
        'ڈ' => "ढ",
        'ڑ' => "ढ़",
        'ک' => "ख",
        'گ' => "घ",
        _ => return None,
    })
}

/// What the last emitted Devanagari character was, for choosing vowel forms.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Prev {
    /// Start of the word.
    Start,
    /// A consonant with no vowel sign yet.
    Consonant,
    /// A vowel (sign or independent).
    Vowel,
}

/// Word-final consonant pairs that are a cluster with no vowel between them (دوست
/// "dost", وقت "waqt", ٹیسٹ "test", بند "band").
fn is_final_cluster(a: char, b: char) -> bool {
    matches!(
        (a, b),
        ('س' | 'ش' | 'ک' | 'ق' | 'ف' | 'ن' | 'ر' | 'ل', 'ت' | 'ٹ' | 'ط')
            | ('ن' | 'ر' | 'ل', 'د' | 'ڈ')
            | ('ن' | 'ر', 'س')
            | ('م', 'پ')
            | ('ن', 'ک' | 'ج' | 'چ')
    )
}

fn word_to_devanagari(w: &[char], out: &mut String) {
    let n = w.len();
    // "-ing" in English loanwords: a final نگ after a consonant, in a word long enough
    // not to be Hindi (میٹنگ "meeting", بکنگ "booking", but رنگ "rang").
    let ing = n >= 4 && w[n - 2] == 'ن' && w[n - 1] == 'گ' && consonant(w[n - 3]).is_some();
    let cluster = n >= 3 && consonant(w[n - 2]).is_some() && is_final_cluster(w[n - 2], w[n - 1]);
    let mut prev = Prev::Start;
    let mut i = 0;
    while i < n {
        let c = w[i];
        if ing && i == n - 2 && prev == Prev::Consonant {
            out.push_str("िंग");
            break;
        }
        let next = w.get(i + 1).copied();
        let at_end = i + 1 == n;
        match c {
            'ا' | 'ع' => match prev {
                // Initial alef/ain carries the vowel that follows it.
                Prev::Start => {
                    let (v, skip) = match next {
                        Some('ی') => ("ए", 1),
                        Some('ے') => ("ए", 1),
                        Some('و') => ("ओ", 1),
                        Some('\u{0650}') => ("इ", 1), // zer
                        Some('\u{064F}') => ("उ", 1), // pesh
                        _ => ("अ", 0),
                    };
                    out.push_str(v);
                    i += skip;
                    prev = Prev::Vowel;
                }
                Prev::Consonant => {
                    out.push('ा');
                    prev = Prev::Vowel;
                }
                // ع after a vowel is silent; ا after a vowel is a new "aa".
                Prev::Vowel => {
                    if c == 'ا' {
                        out.push('आ');
                    }
                }
            },
            'آ' => {
                out.push(if prev == Prev::Consonant { 'ा' } else { 'आ' });
                prev = Prev::Vowel;
            }
            'و' => {
                // Consonant "v" at the start, after a vowel, or before another vowel
                // letter (سوال "sawaal"); otherwise the vowel "o".
                if prev != Prev::Consonant || next.is_some_and(|x| matches!(x, 'ا' | 'آ' | 'ی' | 'ے')) {
                    out.push('व');
                    prev = Prev::Consonant;
                } else {
                    out.push('ो');
                    prev = Prev::Vowel;
                }
            }
            'ی' => {
                if prev != Prev::Consonant {
                    out.push('य');
                    prev = Prev::Consonant;
                } else if next.is_some_and(|x| matches!(x, 'ا' | 'آ' | 'و' | 'ے')) {
                    // دیا → दिया "diya", لیے → लिये
                    out.push_str("िय");
                    prev = Prev::Consonant;
                } else {
                    out.push('ी');
                    prev = Prev::Vowel;
                }
            }
            'ے' => {
                out.push(if prev == Prev::Consonant { 'े' } else { 'ए' });
                prev = Prev::Vowel;
            }
            'ئ' => {
                // Hamza seat: the next vowel letter starts a new syllable (گئی "gai").
                let (v, skip) = match next {
                    Some('ی') => ("ई", 1),
                    Some('ے') => ("ए", 1),
                    Some('و') => ("ओ", 1),
                    Some('ا') => ("आ", 1),
                    _ => ("इ", 0),
                };
                out.push_str(v);
                i += skip;
                prev = Prev::Vowel;
            }
            'ء' => {}
            'ں' => out.push('ं'),
            'ہ' if at_end && prev == Prev::Consonant => {
                // Final he after a consonant is a vowel ("bachcha").
                out.push('ा');
                prev = Prev::Vowel;
            }
            'ھ' => {
                // A stray do-chashmi he (aspirates were taken with their consonant).
                out.push('ह');
                prev = Prev::Consonant;
            }
            '\u{064E}' => {} // zabar: the inherent "a"
            '\u{0650}' => {
                if prev == Prev::Consonant {
                    out.push('ि');
                    prev = Prev::Vowel;
                }
            }
            '\u{064F}' => {
                if prev == Prev::Consonant {
                    out.push('ु');
                    prev = Prev::Vowel;
                }
            }
            '\u{0652}' => {
                if prev == Prev::Consonant {
                    out.push('्');
                }
            }
            '\u{0651}' => {
                // Shadda doubles the previous consonant.
                if prev == Prev::Consonant
                    && let Some(d) = (0..i).rev().find_map(|j| consonant(w[j]))
                {
                    out.push('्');
                    out.push_str(d);
                }
            }
            '\u{0670}' => {
                if prev == Prev::Consonant {
                    out.push('ा');
                    prev = Prev::Vowel;
                }
            }
            '\u{064B}' => out.push('न'), // tanween fatha: "-an" (fauran)
            _ => {
                if let Some(d) = next.filter(|&x| x == 'ھ').and_then(|_| aspirated(c)) {
                    out.push_str(d);
                    i += 1;
                    prev = Prev::Consonant;
                } else if let Some(d) = consonant(c) {
                    out.push_str(d);
                    prev = Prev::Consonant;
                    if cluster && i == n - 2 {
                        out.push('्');
                    }
                }
                // Anything else (other marks) is dropped.
            }
        }
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loanwords::to_roman_hinglish;

    fn roman(urdu: &str) -> String {
        to_roman_hinglish(urdu)
    }

    #[test]
    fn observed_deepgram_cases() {
        assert_eq!(urdu_to_devanagari("تیم"), "तीम");
        assert_eq!(roman("تیم"), "team");
        assert_eq!(roman("ٹیم"), "team");
        for (urdu, en) in [
            ("میٹنگ", "meeting"),
            ("رپورٹ", "report"),
            ("پروجیکٹ", "project"),
            ("ٹیسٹ", "test"),
            ("ڈاکٹر", "doctor"),
            ("ویک", "week"),
        ] {
            assert_eq!(roman(urdu), en, "{urdu}");
        }
        assert_eq!(roman("تیم، اس week کا standup"), "team, is week ka standup");
    }

    #[test]
    fn common_function_words() {
        let cases = [
            ("ہے", "hai"),
            ("ہیں", "hain"),
            ("ہوں", "hoon"),
            ("کیا", "kya"),
            ("نہیں", "nahin"),
            ("اور", "aur"),
            ("یہ", "ye"),
            ("وہ", "wo"),
            ("کا", "ka"),
            ("کی", "ki"),
            ("کے", "ke"),
            ("کو", "ko"),
            ("سے", "se"),
            ("تو", "to"),
            ("بھی", "bhi"),
            ("ہی", "hi"),
            ("مجھے", "mujhe"),
            ("تھا", "tha"),
            ("ٹھیک", "theek"),
            ("ہاں", "haan"),
            ("بہت", "bahut"),
            ("لیکن", "lekin"),
            ("پھر", "phir"),
            ("کچھ", "kuchh"),
            ("ابھی", "abhi"),
            ("بھیج", "bhej"),
            ("دینا", "dena"),
            ("پہلے", "pahle"),
        ];
        for (urdu, want) in cases {
            assert_eq!(roman(urdu), want, "{urdu}");
        }
    }

    #[test]
    fn letter_map() {
        let cases = [
            ("دیا", "दिया"),
            ("کھانا", "खाना"),
            ("گھر", "घर"),
            ("سوال", "सवाल"),
            ("دوست", "दोस्त"),
            ("وقت", "वक़्त"),
            ("بند", "बन्द"),
            ("میٹنگ", "मीटिंग"),
            ("سنگ", "सनग"),
            ("ایک", "एक"),
            ("بچہ", "बचा"),
            ("گئی", "गई"),
        ];
        for (urdu, want) in cases {
            assert_eq!(urdu_to_devanagari(urdu), want, "{urdu}");
        }
    }

    #[test]
    fn punctuation_digits_and_mixed_text() {
        assert_eq!(urdu_to_devanagari("ٹھیک ہے۔ کل؟ ۱۲۳"), "ठीक है. कल? 123");
        assert_eq!(urdu_to_devanagari("kal meeting है"), "kal meeting है");
        assert!(has_urdu("kal تیم"));
        assert!(!has_urdu("kal team कल"));
        assert!(!has_urdu("\u{FEFF}kal"));
    }

    #[test]
    fn nothing_arabic_survives() {
        let s = urdu_to_devanagari("ﷺ ـ ؀ ٍ ﻻ تیم");
        assert!(!has_urdu(&s), "{s:?}");
    }
}
