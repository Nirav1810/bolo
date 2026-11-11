//! The settings window's "Test" button for an API key: one cheap authenticated request
//! to the provider (listing models or projects), which a bad key fails with 401 or 403.

use std::time::Duration;

use serde::Serialize;

/// The request that checks a key: a GET to `url` with `headers`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Probe {
    pub url: &'static str,
    pub headers: Vec<(&'static str, String)>,
}

/// How to check `provider`'s `key`. `None` for a provider with no cheap check (Sarvam:
/// its key is checked on the first dictation).
pub fn probe(provider: &str, key: &str) -> Option<Probe> {
    let key = key.trim().to_owned();
    Some(match provider {
        "anthropic" => Probe {
            url: "https://api.anthropic.com/v1/models",
            headers: vec![("x-api-key", key), ("anthropic-version", "2023-06-01".into())],
        },
        "deepgram" => Probe {
            url: "https://api.deepgram.com/v1/projects",
            headers: vec![("authorization", format!("Token {key}"))],
        },
        // Usage keys: a request only a key that may read usage passes.
        "anthropic-admin" => Probe {
            url: "https://api.anthropic.com/v1/organizations/api_keys?limit=1",
            headers: vec![("x-api-key", key), ("anthropic-version", "2023-06-01".into())],
        },
        "deepgram-admin" => Probe {
            url: "https://api.deepgram.com/v1/projects",
            headers: vec![("authorization", format!("Token {key}"))],
        },
        "groq" => Probe {
            url: "https://api.groq.com/openai/v1/models",
            headers: vec![("authorization", format!("Bearer {key}"))],
        },
        _ => return None,
    })
}

/// What the test found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "result", rename_all = "kebab-case")]
pub enum Tested {
    Ok,
    /// The provider refused the key.
    Refused,
    /// No answer, or an unexpected one.
    Failed {
        message: String,
    },
    /// This provider can't be checked ahead of time.
    Untestable,
}

/// The result for an HTTP status.
pub fn verdict(status: u16) -> Tested {
    match status {
        200..=299 => Tested::Ok,
        401 | 403 => Tested::Refused,
        s => Tested::Failed { message: format!("the provider answered HTTP {s}") },
    }
}

/// Check `key` for `provider`.
pub async fn test(provider: &str, key: &str) -> Tested {
    let Some(p) = probe(provider, key) else { return Tested::Untestable };
    let client = match bolo_stt::cloud::http_client() {
        Ok(c) => c,
        Err(e) => return Tested::Failed { message: e.to_string() },
    };
    let mut req = client.get(p.url).timeout(Duration::from_secs(10));
    for (name, value) in p.headers {
        req = req.header(name, value);
    }
    match req.send().await {
        Ok(resp) => verdict(resp.status().as_u16()),
        Err(e) => Tested::Failed { message: if e.is_timeout() { "no answer in 10 s".into() } else { e.to_string() } },
    }
}

#[cfg(test)]
mod tests {
    use super::{Tested, probe, verdict};

    #[test]
    fn each_provider_sends_its_key_its_own_way() {
        let a = probe("anthropic", " sk-ant ").unwrap();
        assert_eq!(a.headers[0], ("x-api-key", "sk-ant".to_owned()));
        assert_eq!(probe("deepgram", "dg").unwrap().headers[0].1, "Token dg");
        assert_eq!(probe("groq", "gsk").unwrap().headers[0].1, "Bearer gsk");
        assert!(probe("sarvam", "k").is_none());
        assert!(probe("deepgram", "k").unwrap().url.starts_with("https://"));
    }

    #[test]
    fn statuses() {
        assert_eq!(verdict(200), Tested::Ok);
        assert_eq!(verdict(401), Tested::Refused);
        assert_eq!(verdict(403), Tested::Refused);
        assert!(matches!(verdict(500), Tested::Failed { .. }));
    }
}
