//! `bolo cleanup-eval`: run Layer-2 LLM cleanup over transcripts saved by a bench
//! run, then score before vs after against the references. No STT calls are made.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use bolo_format::llm::{self, CleanupRequest, Script};
use serde::{Deserialize, Serialize};

use crate::metrics::{ErrorCount, cer, percentile, wer};

#[derive(Deserialize)]
struct Row {
    engine: String,
    clip: String,
    category: String,
    hypothesis: String,
    reference: String,
}

#[derive(Serialize)]
struct OutRow {
    engine: String,
    clip: String,
    category: String,
    wer_before: f64,
    wer_after: f64,
    latency_ms: f64,
    cache_read_tokens: u64,
    before: String,
    after: String,
    reference: String,
}

/// Score text the way the app would show it: romanize any Devanagari or Urdu script
/// first.
fn romanized(text: &str) -> String {
    if bolo_format::translit::needs_romanizing(text) {
        bolo_format::loanwords::to_roman_hinglish(text)
    } else {
        text.to_owned()
    }
}

pub async fn run(csv_path: &Path, engines: &[String], out: &Path, aliases: &HashMap<String, String>) -> Result<()> {
    let key = crate::env_nonempty("ANTHROPIC_API_KEY").ok_or_else(|| anyhow!("put ANTHROPIC_API_KEY in .env"))?;
    let client = bolo_stt::cloud::http_client()?;
    let mut rdr = csv::Reader::from_path(csv_path).with_context(|| format!("reading {}", csv_path.display()))?;
    let rows: Vec<Row> = rdr
        .deserialize()
        .collect::<Result<Vec<Row>, _>>()?
        .into_iter()
        .filter(|r| engines.iter().any(|e| r.engine.contains(e.as_str())))
        .collect();
    std::fs::create_dir_all(out)?;

    type Acc = (ErrorCount, ErrorCount, ErrorCount, ErrorCount); // wer before/after, cer before/after
    let mut acc: BTreeMap<(String, String), Acc> = BTreeMap::new();
    let mut lat: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let mut cost: BTreeMap<String, f64> = BTreeMap::new();
    let mut cache_hits: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut out_rows = Vec::new();

    for r in &rows {
        let req = CleanupRequest {
            transcript: &r.hypothesis,
            script: Script::Roman,
            tone: "neutral",
            app: "Notes",
            dictionary: &[],
            context: None,
        };
        // Generous timeout for measurement; the app enforces 1.5 s and falls back.
        let res = match llm::cleanup(&client, &key, &req, Duration::from_secs(10)).await {
            Ok(x) => x,
            Err(e) => {
                eprintln!("  {} {}: {e}", r.engine, r.clip);
                continue;
            }
        };
        let before = romanized(&r.hypothesis);
        let after = romanized(&res.text);
        let (wb, wa) = (wer(&r.reference, &before, aliases), wer(&r.reference, &after, aliases));
        let (cb, ca) = (cer(&r.reference, &before), cer(&r.reference, &after));
        let a = acc.entry((r.engine.clone(), r.category.clone())).or_default();
        a.0.add(wb);
        a.1.add(wa);
        a.2.add(cb);
        a.3.add(ca);
        lat.entry(r.engine.clone()).or_default().push(res.latency_ms);
        *cost.entry(r.engine.clone()).or_default() += res.cost_usd();
        let h = cache_hits.entry(r.engine.clone()).or_default();
        h.0 += usize::from(res.cache_read_tokens > 0);
        h.1 += 1;
        eprintln!(
            "  {:<24} {:<44} {:>5.0} ms  WER {:>5.1}% → {:>5.1}%  {}",
            r.engine,
            r.clip,
            res.latency_ms,
            wb.rate() * 100.0,
            wa.rate() * 100.0,
            after
        );
        out_rows.push(OutRow {
            engine: r.engine.clone(),
            clip: r.clip.clone(),
            category: r.category.clone(),
            wer_before: wb.rate(),
            wer_after: wa.rate(),
            latency_ms: res.latency_ms,
            cache_read_tokens: res.cache_read_tokens,
            before,
            after,
            reference: r.reference.clone(),
        });
    }

    let mut w = csv::Writer::from_path(out.join("cleanup.csv"))?;
    for r in &out_rows {
        w.serialize(r)?;
    }
    w.flush()?;

    let mut md = String::from(
        "| engine | category | WER before | WER after | CER before | CER after |\n|---|---|---|---|---|---|\n",
    );
    for ((eng, cat), (wb, wa, cb, ca)) in &acc {
        md += &format!(
            "| {eng} | {cat} | {:.1}% | {:.1}% | {:.1}% | {:.1}% |\n",
            wb.rate() * 100.0,
            wa.rate() * 100.0,
            cb.rate() * 100.0,
            ca.rate() * 100.0
        );
    }
    md += "\n| engine | calls | p50 ms | p95 ms | ≤1.5 s | cache hits | cost USD |\n|---|---|---|---|---|---|---|\n";
    for (eng, l) in &lat {
        let within = l.iter().filter(|x| **x <= 1500.0).count();
        let (hits, n) = cache_hits.get(eng).copied().unwrap_or_default();
        md += &format!(
            "| {eng} | {} | {:.0} | {:.0} | {}/{} | {hits}/{n} | {:.4} |\n",
            l.len(),
            percentile(l, 50.0).unwrap_or(0.0),
            percentile(l, 95.0).unwrap_or(0.0),
            within,
            l.len(),
            cost.get(eng).copied().unwrap_or(0.0)
        );
    }
    std::fs::write(out.join("summary.md"), &md)?;
    println!("{md}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(csv_text: &str) -> Vec<Row> {
        csv::Reader::from_reader(csv_text.as_bytes()).deserialize().collect::<Result<_, _>>().unwrap()
    }

    #[test]
    fn loads_old_and_new_clips_csv() {
        // Before M2 ticket 02, and after (formatted and polish columns added).
        let old = "engine,clip,category,audio_s,latency_ms,rtf,wer,cer,devanagari,cached,\
                   hypothesis,hypothesis_scored,reference\n\
                   local:x,en/01.wav,en,2.9,194.8,0.07,0.0,0.0,false,false,Sounds good,,\"Sounds good.\"\n";
        let new = "engine,clip,category,audio_s,latency_ms,rtf,wer,wer_formatted,wer_polished,\
                   wer_formatted_polished,cer,devanagari,cached,hypothesis,hypothesis_scored,polished,\
                   reference,reference_formatted\n\
                   local:x,en/01.wav,en,2.9,194.8,0.07,0.0,0.0,,,0.0,false,false,Sounds good,,,\
                   \"Sounds good.\",\"Sounds good.\"\n";
        for text in [old, new] {
            let r = rows(text);
            assert_eq!(r.len(), 1);
            assert_eq!(
                (r[0].engine.as_str(), r[0].hypothesis.as_str(), r[0].reference.as_str()),
                ("local:x", "Sounds good", "Sounds good.")
            );
        }
    }
}
