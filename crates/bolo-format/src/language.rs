//! Which language a dictation was in, judged from the Hindi pass of an Auto-mode dual
//! decode (ADR-0014).
//!
//! When Nemotron is forced to Hindi, Hindi speech comes out as Devanagari full of
//! function words (है, का, में, नहीं …). English speech comes out as English words spelled
//! in Devanagari (कैन यू सेंड मी द रिपोर्ट), which almost never contain them. So the share
//! of Hindi function words in the Hindi pass separates the two cleanly.
//!
//! The list sits next to the loanword stoplist ([`crate::loanwords`]) but is much
//! narrower: only grammatical words, and none that an English word commonly turns into
//! when written in Devanagari (इस "is", इन "in", सो "so", मी "me").

use std::collections::HashSet;
use std::sync::OnceLock;

/// Hindi function words: auxiliaries, pronouns, postpositions, conjunctions, particles,
/// question words, and the commonest light-verb forms.
const HINDI_FUNCTION_WORDS: &str = "
है हैं था थी थे हो हूँ हूं
मैं मुझे मुझको मेरा मेरी मेरे हम हमें हमारा हमारी हमारे तुम तुम्हें तुम्हारा तुम्हारी तू तेरा तेरी
आप आपका आपकी आपके आपको वह वो उसे उसको उसका उसकी उसके उन्हें उनका उनकी उनके
यह इसे इसको इसका इसकी इसके
का की के को से में पर तक ने लिए लिये वाला वाली वाले
और या लेकिन मगर फिर तो भी ही कि अगर जब तब नहीं ना मत हाँ हां जी
क्या क्यों कैसे कैसा कैसी कब कहाँ कहां कौन कितना कितने
रहा रही रहे गया गई गए दिया दी दिए किया
";

/// Fewer words than this is too little evidence either way ("ok", "send it").
///
/// Tuned 2026-11-09 on the 40-clip set, Nemotron forced `hi`: the shortest clips are
/// 3 words in both categories and every one of them was classified correctly.
pub const MIN_WORDS: usize = 3;
/// At or below this share of function words the dictation is English.
///
/// Tuned 2026-11-09: English clips decoded as Hindi peak at 0.071 (1 of 14 words, a
/// stray का); 18 of 20 have none.
pub const ENGLISH_MAX_DENSITY: f32 = 0.10;
/// At or above this share the dictation is Hinglish.
///
/// Tuned 2026-11-09: Hinglish clips bottom out at 0.25 and median 0.40. Between the
/// two thresholds is the confidence margin, where the dictation counts as ambiguous.
pub const HINGLISH_MIN_DENSITY: f32 = 0.20;

/// The language a dictation was spoken in, as far as the detector can tell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detected {
    English,
    Hinglish,
    /// Too short, or inside the confidence margin: the caller decides (ADR-0014 reuses
    /// the previous dictation's language).
    Ambiguous,
}

/// Word and function-word counts in a Hindi-pass transcript.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HindiEvidence {
    pub words: usize,
    pub function_words: usize,
}

impl HindiEvidence {
    pub fn density(self) -> f32 {
        if self.words == 0 { 0.0 } else { self.function_words as f32 / self.words as f32 }
    }

    pub fn detect(self) -> Detected {
        if self.words < MIN_WORDS {
            return Detected::Ambiguous;
        }
        let d = self.density();
        if d >= HINGLISH_MIN_DENSITY {
            Detected::Hinglish
        } else if d <= ENGLISH_MAX_DENSITY {
            Detected::English
        } else {
            Detected::Ambiguous
        }
    }
}

fn function_words() -> &'static HashSet<&'static str> {
    static SET: OnceLock<HashSet<&'static str>> = OnceLock::new();
    SET.get_or_init(|| HINDI_FUNCTION_WORDS.split_whitespace().collect())
}

fn words(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| c.is_whitespace() || (c.is_ascii_punctuation() && c != '\'') || matches!(c, '।' | '॥'))
        .filter(|w| !w.is_empty())
}

/// Count words and Hindi function words in the Hindi pass's output. Urdu-script words
/// are mapped to Devanagari first ([`crate::urdu`]), so ہے counts like है.
pub fn hindi_evidence(hindi_pass: &str) -> HindiEvidence {
    let set = function_words();
    let mapped = crate::urdu::urdu_to_devanagari(hindi_pass);
    words(&mapped).fold(HindiEvidence::default(), |mut e, w| {
        e.words += 1;
        e.function_words += usize::from(set.contains(w));
        e
    })
}

/// Detect the dictation's language from the Hindi pass's output.
pub fn detect(hindi_pass: &str) -> Detected {
    hindi_evidence(hindi_pass).detect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urdu_script_hindi_is_hinglish() {
        assert_eq!(detect("کل meeting ہے نا، agenda بھیج دینا"), Detected::Hinglish);
    }

    #[test]
    fn english_spelled_in_devanagari_is_english() {
        // Real Nemotron `hi` output for "Can you send me the report by Friday".
        assert_eq!(detect("कैन यू सेंड मी द रिपोर्ट बाइ फ्रेड"), Detected::English);
        // "is", "in", "so" written in Devanagari are not counted.
        assert_eq!(detect("द इनवॉइस नंबर इस फोर फाइव सो इन द एंड"), Detected::English);
    }

    #[test]
    fn hindi_with_function_words_is_hinglish() {
        assert_eq!(detect("कल मीटिंग है ना अचेंडा भेज देना"), Detected::Hinglish);
        assert_eq!(detect("हाँ, ठीक है, कल मिलता है"), Detected::Hinglish);
        assert_eq!(detect("सर्वर फिर से डाउन हो गया है।  कोई चेक कर सकता है क्या"), Detected::Hinglish);
    }

    #[test]
    fn short_or_marginal_dictations_are_ambiguous() {
        assert_eq!(detect(""), Detected::Ambiguous);
        assert_eq!(detect("ओके"), Detected::Ambiguous);
        assert_eq!(detect("सेंड इट"), Detected::Ambiguous);
        // 1 function word in 7 (0.14) is inside the margin.
        assert_eq!(detect("प्लीज सेंड द रिपोर्ट टू का क्लाइंट"), Detected::Ambiguous);
    }

    #[test]
    fn evidence_counts_words_and_ignores_punctuation() {
        let e = hindi_evidence("मैं थोड़ी देर में कॉल करता हूँ।");
        assert_eq!(e, HindiEvidence { words: 7, function_words: 3 });
        assert!((e.density() - 3.0 / 7.0).abs() < 1e-6);
    }
}
