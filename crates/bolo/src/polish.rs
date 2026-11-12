//! `--polish` for `bolo run` and `bolo replay`: the Layer 2 Claude rewrite applied to
//! each bench output, so the bench can score Layer 1 + polish. Each call costs API
//! money, so nothing here runs unless the flag is given.

use std::time::Duration;

use anyhow::{Result, anyhow};
use bolo_format::llm::{self, CleanupRequest, Script};

/// Generous, so slow calls are measured rather than dropped. The app's deadline is 1.5 s.
const TIMEOUT: Duration = Duration::from_secs(10);

pub struct Polisher {
    client: reqwest::Client,
    key: String,
    calls: usize,
    failures: usize,
    cost_usd: f64,
}

impl Polisher {
    /// Fails when `ANTHROPIC_API_KEY` is missing, before any clip is run.
    pub fn from_env() -> Result<Self> {
        let key = crate::env_nonempty("ANTHROPIC_API_KEY")
            .ok_or_else(|| anyhow!("--polish needs ANTHROPIC_API_KEY in .env"))?;
        Ok(Self { client: bolo_stt::cloud::http_client()?, key, calls: 0, failures: 0, cost_usd: 0.0 })
    }

    /// The polished text. A failed call keeps the Layer 1 text, as the app does.
    pub async fn polish(&mut self, text: &str) -> String {
        if text.trim().is_empty() {
            return String::new();
        }
        let req = CleanupRequest {
            transcript: text,
            script: Script::Roman,
            tone: "neutral",
            app: "Notes",
            dictionary: &[],
            triggers: &[],
            context: None,
        };
        self.calls += 1;
        match llm::cleanup(&self.client, &self.key, &req, TIMEOUT).await {
            Ok(r) => {
                self.cost_usd += r.cost_usd();
                r.text
            }
            Err(e) => {
                eprintln!("    polish failed, keeping the Layer 1 text: {e}");
                self.failures += 1;
                text.to_owned()
            }
        }
    }

    pub fn report(&self) -> String {
        format!("polish: {} calls, {} failed (Layer 1 text kept), ${:.4}", self.calls, self.failures, self.cost_usd)
    }
}
