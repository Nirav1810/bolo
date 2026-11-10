//! Latency gate (ADR-0015): the whole Layer 1 pipeline stays under 1 ms p95 per
//! dictation on the 40 bench clips' transcripts.
//!
//! The transcripts are the clips' verbatim references, in `testdata/clips`. Each is run
//! as written and as a bare engine would give it (lowercase, no punctuation), in every
//! format context. This is a debug build in CI, so the release-build margin is far larger.

use std::path::Path;
use std::time::Instant;

use bolo_format::rules::{self, FormatContext, Language};

const BUDGET_MS: f64 = 1.0;
const ROUNDS: usize = 20;

fn transcripts() -> Vec<(Language, String)> {
    let clips = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/clips");
    let mut out = Vec::new();
    for (dir, language) in [("en", Language::English), ("hinglish", Language::Hinglish)] {
        let entries = std::fs::read_dir(clips.join(dir)).unwrap_or_else(|e| panic!("{dir}: {e}"));
        for path in entries.filter_map(|e| e.ok().map(|e| e.path())) {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            if name.ends_with(".txt") && !name.ends_with(".formatted.txt") {
                let text = std::fs::read_to_string(&path).unwrap_or_default();
                let bare: String = text
                    .to_lowercase()
                    .chars()
                    .filter(|c| c.is_alphanumeric() || c.is_whitespace() || *c == '\'')
                    .collect();
                out.push((language, text.trim().to_owned()));
                out.push((language, bare.trim().to_owned()));
            }
        }
    }
    out
}

#[test]
fn pipeline_is_under_1ms_p95_on_the_bench_transcripts() {
    let texts = transcripts();
    assert_eq!(texts.len(), 80, "expected the 40 bench clips' transcripts");
    let contexts = [FormatContext::plain(), FormatContext::markdown(), FormatContext::code()];
    let mut times = Vec::with_capacity(ROUNDS * texts.len() * contexts.len());
    for _ in 0..ROUNDS {
        for (language, text) in &texts {
            for ctx in contexts {
                let t = Instant::now();
                let out = rules::apply(text, *language, ctx);
                times.push(t.elapsed().as_secs_f64() * 1e3);
                assert!(!out.is_empty());
            }
        }
    }
    times.sort_by(f64::total_cmp);
    let p95 = times[times.len() * 95 / 100];
    let max = times[times.len() - 1];
    eprintln!("rules pipeline: p95 {p95:.4} ms, max {max:.4} ms over {} runs", times.len());
    assert!(p95 < BUDGET_MS, "rules pipeline p95 {p95:.3} ms is over the {BUDGET_MS} ms budget");
}
