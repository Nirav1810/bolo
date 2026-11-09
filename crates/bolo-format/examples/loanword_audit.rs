//! Lists every Devanagari word that the loanword pass turns into English, from the
//! `hypothesis` column of a bench `clips.csv`. Usage:
//!   cargo run -p bolo-format --example loanword_audit -- bench-results/clips.csv [engine-substring]
use std::collections::BTreeMap;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().unwrap_or_else(|| "bench-results/clips.csv".into());
    let filter = args.next().unwrap_or_default();
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let mut hits: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut total = 0usize;
    for line in text.lines().skip(1).filter(|l| l.contains(&filter)) {
        for w in line.split(|c: char| !(('\u{0900}'..='\u{097F}').contains(&c))).filter(|w| !w.is_empty()) {
            total += 1;
            if let Some(en) = bolo_format::loanwords::english_for(w) {
                *hits.entry((w.to_owned(), en.to_owned())).or_default() += 1;
            }
        }
    }
    for ((dev, en), n) in &hits {
        println!("{n:>3}  {dev} → {en}");
    }
    println!("{} distinct conversions out of {total} Devanagari word tokens", hits.len());
}
