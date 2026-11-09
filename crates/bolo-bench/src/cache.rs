//! Cloud response cache keyed by `(engine id, options hash, sha256(audio))`, so reruns
//! cost nothing. Stored as one small JSON per entry under `<cache>/<engine>/`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cached {
    pub text: String,
    /// Latency measured when the response was first fetched.
    pub latency_ms: f64,
}

pub struct Cache {
    dir: PathBuf,
}

impl Cache {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn key(audio: &[f32], opts_fingerprint: &str) -> String {
        let mut h = Sha256::new();
        for s in audio {
            h.update(s.to_le_bytes());
        }
        h.update(opts_fingerprint.as_bytes());
        hex::encode(h.finalize())
    }

    fn path(&self, engine: &str, key: &str) -> PathBuf {
        let safe: String =
            engine.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '.' { c } else { '_' }).collect();
        self.dir.join(safe).join(format!("{key}.json"))
    }

    pub fn get(&self, engine: &str, key: &str) -> Option<Cached> {
        let bytes = std::fs::read(self.path(engine, key)).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    pub fn put(&self, engine: &str, key: &str, value: &Cached) -> std::io::Result<()> {
        let p = self.path(engine, key);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(p, serde_json::to_vec_pretty(value).map_err(std::io::Error::other)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_key_sensitivity() {
        let dir = std::env::temp_dir().join(format!("bolo-bench-cache-{}", std::process::id()));
        let c = Cache::new(&dir);
        let k = Cache::key(&[0.1, 0.2], "lang=auto");
        assert_ne!(k, Cache::key(&[0.1, 0.2], "lang=en"));
        assert!(c.get("groq:whisper-large-v3-turbo", &k).is_none());
        c.put("groq:whisper-large-v3-turbo", &k, &Cached { text: "hi".into(), latency_ms: 420.0 }).unwrap();
        assert_eq!(c.get("groq:whisper-large-v3-turbo", &k).unwrap().text, "hi");
        std::fs::remove_dir_all(dir).ok();
    }
}
