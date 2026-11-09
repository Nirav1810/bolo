//! Bring-your-own-key cloud engines. One shared `reqwest::Client` (keep-alive, HTTP/2
//! via ALPN, rustls + ring) is reused across providers so `prewarm()` can open the TLS
//! connection while the user is still speaking.

use std::sync::Once;
use std::time::Duration;

use crate::{Result, SttError};

pub mod deepgram;
pub mod deepgram_stream;
pub mod groq;
pub mod sarvam;

pub use deepgram::DeepgramEngine;
pub use deepgram_stream::DeepgramStreamEngine;
pub use groq::GroqEngine;
pub use sarvam::SarvamEngine;

static INSTALL_PROVIDER: Once = Once::new();

/// Make ring the process-wide rustls crypto provider (ADR-004).
pub(crate) fn install_ring() {
    INSTALL_PROVIDER.call_once(|| {
        // Err means another provider was installed first, which is fine.
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

/// Build the shared HTTP client. Call once per process and clone the result (cheap, `Arc`).
pub fn http_client() -> Result<reqwest::Client> {
    install_ring();
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(3))
        .pool_idle_timeout(Duration::from_secs(90))
        .tcp_keepalive(Duration::from_secs(30))
        .user_agent(concat!("bolo/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| SttError::Network(e.to_string()))
}

/// Open (or reuse) a pooled connection to `url` so the real request skips DNS+TCP+TLS.
/// Any HTTP status is fine — only the connection matters.
pub(crate) async fn warm_connection(client: &reqwest::Client, url: &str) -> Result<()> {
    client.head(url).timeout(Duration::from_secs(3)).send().await.map(|_| ()).map_err(map_reqwest)
}

pub(crate) fn map_reqwest(e: reqwest::Error) -> SttError {
    if e.is_timeout() {
        SttError::Timeout(Duration::ZERO)
    } else if e.is_decode() {
        SttError::Decode(e.to_string())
    } else {
        // reqwest errors may include the URL (never the key: keys go in headers).
        SttError::Network(e.without_url().to_string())
    }
}

/// Turn a non-success response into an error, keeping a short body snippet for logs.
pub(crate) async fn check_status(resp: reqwest::Response) -> Result<reqwest::Response> {
    let status = resp.status();
    if status.is_success() {
        return Ok(resp);
    }
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err(SttError::Auth);
    }
    let body = resp.text().await.unwrap_or_default();
    let body: String = body.chars().take(300).collect();
    Err(SttError::Status { status: status.as_u16(), body })
}

/// Dictionary terms joined into a prompt, capped by characters (Groq allows 224 tokens).
pub(crate) fn join_terms(terms: &[String], max_chars: usize) -> String {
    let mut out = String::new();
    for t in terms {
        let t = t.trim();
        if t.is_empty() {
            continue;
        }
        if out.len() + t.len() + 2 > max_chars {
            break;
        }
        if !out.is_empty() {
            out.push_str(", ");
        }
        out.push_str(t);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_terms_caps_length() {
        let terms: Vec<String> = ["Bolo", "  ", "Tauri", "sherpa-onnx"].iter().map(|s| s.to_string()).collect();
        assert_eq!(join_terms(&terms, 100), "Bolo, Tauri, sherpa-onnx");
        assert_eq!(join_terms(&terms, 12), "Bolo, Tauri");
    }
}
