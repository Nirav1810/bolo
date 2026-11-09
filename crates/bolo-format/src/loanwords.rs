//! Restore English spelling for English words that an ASR engine wrote in Devanagari:
//! ऑफिस → office, डॉक्टर → doctor, रिपोर्ट → report.
//!
//! Matching works on coarse phonetic keys. Both sides map into the same alphabet:
//! Devanagari via [`crate::translit`], English via CMUdict pronunciations at build time
//! (`scripts/build_loanword_lexicon.py` → `data/loanwords.tsv`).
//!
//! A candidate must share the consonant skeleton exactly, and its vowels may differ by
//! at most a small budget that grows with word length. Short words get no slack because
//! that is where Hindi and English collide (बात "baat" vs "but").
//!
//! Common Hindi words are protected by a stoplist, compared by phonetic key so spelling
//! variants are covered too.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use crate::translit::{devanagari_to_roman_with, phonetic_key};

const LEXICON_TSV: &str = include_str!("../data/loanwords.tsv");

/// Frequent Hindi words that must never be "corrected" into English. General-purpose
/// list (function words, pronouns, auxiliaries, common verbs/nouns/adjectives), not
/// tuned to any test set.
const HINDI_STOPWORDS: &str = "
है हैं था थी थे हो हूँ हूं होगा होगी होंगे होना हुआ हुई हुए रहा रही रहे रहना
मैं मुझे मेरा मेरी मेरे हम हमें हमारा हमारी हमारे तुम तुम्हें तुम्हारा तुम्हारी तू तेरा तेरी
आप आपका आपकी आपके आपको वह वो उसे उसका उसकी उसके उन्हें उनका उनकी उनके यह ये इसे इसका इसकी इसके
इन्हें इनका इनकी कौन किसका किसकी किसे क्या क्यों कैसे कैसा कैसी कब कहाँ कहां किधर कितना कितनी कितने
का की के को से में पर तक ने लिए लिये साथ बाद पहले बिना वाला वाली वाले जैसा जैसे जैसी तरह
और या लेकिन पर मगर फिर तो भी ही कि अगर तब जब तभी जबकि इसलिए क्योंकि वरना नहीं ना मत हाँ हां जी
कर करो करना करता करती करते किया किये किए करेगा करेंगे करूँ करूं कराना
जा जाओ जाना जाता जाती जाते गया गई गए जाएगा जाऊंगा जाऊँगा
आ आओ आना आता आती आते आया आई आए आएगा आऊंगा आऊँगा
दे दो देना देता देती देते दिया दी दिए देगा लेना ले लो लेता लेती लेते लिया ली लिए
सकता सकती सकते सका सकी चाहिए चाहता चाहती चाहते बोल बोलो बोला बोली बोलना कहा कहो कहना
देख देखो देखा देखी देखना मिल मिलो मिला मिली मिलना मिलते चल चलो चला चली चलना चलते
भेज भेजो भेजा भेजना भेजी बना बनाओ बनाना बनाया बन बनी खोल खोलो खोला बंद रुक रुको रुका
छोड़ छोड़ो छोड़ा डाल डालो डाला पूछ पूछो पूछा जान जानो जाना मान मानो माना भूल भूलो भूला
बता बताओ बताना बताया सोच सोचा सोचो लग लगा लगी लगता लगती लगे रख रखो रखा रखना
पता समझ समझा समझो सुन सुनो सुना पढ़ पढ़ो लिख लिखो खा खाओ खाया पी पियो बैठ बैठो उठ उठो
घर काम बात बातें दिन रात समय वक्त टाइम पैसा पैसे लोग लोगों आदमी औरत बच्चा बच्चे भाई बहन
माँ मां मम्मी पापा दोस्त यार पानी खाना शाम सुबह दोपहर कल आज अभी परसों साल महीना हफ्ता हफ़्ता
अच्छा अच्छी अच्छे बुरा बड़ा बड़ी बड़े छोटा छोटी छोटे नया नई नए पुराना पुरानी पुराने सही गलत ग़लत
ज़्यादा ज्यादा कम बहुत थोड़ा थोड़ी सब सारे कुछ कोई कई हर एक दो तीन चार पांच पाँच छह सात आठ नौ दस
देर जल्द पसंद नापसंद मिलता मिलती बोलते बोलता बोलती सब्जी सब्ज़ी दूध रोटी चावल दाल चाय
ख़ुश खुश दुखी परेशान तैयार ज़रूर जरूर शायद बिल्कुल सचमुच वैसे ऐसे ऐसा ऐसी वैसा वैसी जितना उतना
लड़का लड़की बेटा बेटी पति पत्नी शादी परिवार गाड़ी रास्ता सड़क शहर गांव गाँव दुकान बाज़ार बाजार
सवाल जवाब मतलब ख़बर खबर कहानी गाना फ़िल्म किताब कपड़े जूते दवा दवाई बीमार इलाज
अंदर बाहर ऊपर नीचे आगे पीछे पास दूर यहाँ यहां वहाँ वहां ठीक बस सच झूठ प्यार दिल
";

struct Entry {
    key: &'static str,
    word: &'static str,
    tier: u8,
}

struct Lexicon {
    by_skeleton: HashMap<&'static str, Vec<Entry>>,
    stop_keys: HashSet<String>,
}

fn lexicon() -> &'static Lexicon {
    static LEX: OnceLock<Lexicon> = OnceLock::new();
    LEX.get_or_init(|| {
        let mut by_skeleton: HashMap<&'static str, Vec<Entry>> = HashMap::new();
        for line in LEXICON_TSV.lines().filter(|l| !l.starts_with('#')) {
            let mut f = line.split('\t');
            if let (Some(skel), Some(key), Some(word), Some(tier)) = (f.next(), f.next(), f.next(), f.next()) {
                let tier = tier.parse().unwrap_or(u8::MAX);
                by_skeleton.entry(skel).or_default().push(Entry { key, word, tier });
            }
        }
        let stop_keys = HINDI_STOPWORDS.split_whitespace().map(phonetic_key).collect();
        Lexicon { by_skeleton, stop_keys }
    })
}

fn skeleton(key: &str) -> String {
    key.chars().filter(char::is_ascii_lowercase).collect()
}

/// Vowel groups between consonants: "dOktAr" → ["", "O", "", "A", ""].
fn vowel_groups(key: &str) -> Vec<&str> {
    key.split(|c: char| c.is_ascii_lowercase()).collect()
}

fn levenshtein(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, x) in a.iter().enumerate() {
        let mut cur = vec![i + 1; b.len() + 1];
        for (j, y) in b.iter().enumerate() {
            cur[j + 1] = (prev[j] + usize::from(x != y)).min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        prev = cur;
    }
    prev[b.len()]
}

/// Total vowel edits between two keys with the same consonant skeleton.
fn vowel_distance(a: &str, b: &str) -> usize {
    vowel_groups(a).iter().zip(vowel_groups(b)).map(|(x, y)| levenshtein(x, y)).sum()
}

/// Allowed vowel edits by number of consonants.
fn budget(consonants: usize) -> usize {
    match consonants {
        0..=2 => 0,
        3 | 4 => 1,
        _ => 2,
    }
}

/// The word-final vowel must match exactly: Hindi inflections (-te, -ta, -ti, -i) are
/// real sounds, so बोलते ≠ bolt and सब्जी ≠ subs.
fn same_final_vowel(a: &str, b: &str) -> bool {
    vowel_groups(a).last() == vowel_groups(b).last()
}

/// English spelling for one Devanagari word, if it is confidently an English loanword.
pub fn english_for(devanagari_word: &str) -> Option<&'static str> {
    let lex = lexicon();
    let key = phonetic_key(devanagari_word);
    if lex.stop_keys.contains(&key) {
        return None;
    }
    let skel = skeleton(&key);
    if skel.len() < 2 || key.chars().count() < 3 {
        return None;
    }
    let max = budget(skel.len());
    // Two-consonant words are where Hindi and English collide most (भेज "bhej" = badge),
    // so only very common English words (SCOWL tiers 10/20) qualify there.
    let max_tier = if skel.len() <= 2 { 20 } else { u8::MAX };
    lex.by_skeleton
        .get(skel.as_str())?
        .iter()
        .filter(|e| e.tier <= max_tier && same_final_vowel(&key, e.key))
        .map(|e| (vowel_distance(&key, e.key), e.tier, e.word.len(), e))
        .filter(|(d, ..)| *d <= max)
        .min_by_key(|(d, tier, len, _)| (*d, *tier, *len))
        .map(|(.., e)| e.word)
}

/// Devanagari → Roman Hinglish with English loanwords restored:
/// "मुझे आज ऑफिस से जल्दी निकलना है" → "mujhe aaj office se jaldi nikalna hai".
pub fn to_roman_hinglish(text: &str) -> String {
    devanagari_to_roman_with(text, |w| english_for(w).map(str::to_owned))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restores_common_loanwords() {
        for (dev, en) in [
            ("ऑफिस", "office"),
            ("डॉक्टर", "doctor"),
            ("रिपोर्ट", "report"),
            ("सर्वर", "server"),
            ("फाइनल", "final"),
            ("मीटिंग", "meeting"),
            ("अपॉइंटमेंट", "appointment"),
            ("प्रोजेक्ट", "project"),
            ("शेयर", "share"),
            ("सेंटर", "centre"),
            ("फ्राइडे", "Friday"),
        ] {
            assert_eq!(english_for(dev), Some(en), "{dev}");
        }
    }

    #[test]
    fn leaves_hindi_alone() {
        for w in [
            "बात",
            "पता",
            "कल",
            "मैं",
            "में",
            "है",
            "समझ",
            "घर",
            "काम",
            "जल्दी",
            "निकलना",
            "भेज",
            "हम",
            "बोलते",
            "मिलता",
            "पसंद",
            "देर",
            "सब्जी",
        ] {
            assert_eq!(english_for(w), None, "{w}");
        }
    }

    #[test]
    fn sentence() {
        assert_eq!(
            to_roman_hinglish("मुझे आज ऑफिस से जल्दी निकलना है, डॉक्टर का अपॉइंटमेंट है।"),
            "mujhe aaj office se jaldi nikalna hai, doctor ka appointment hai."
        );
    }
}
