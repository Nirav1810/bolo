//! Ordered fallback chain, e.g. groq → deepgram → local. Each engine gets its own
//! timeout (default 2.5 s for cloud); on error or timeout the next engine is tried.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use crate::{Result, SttEngine, SttError, SttOptions, SttStream, Transcript};

pub const DEFAULT_CLOUD_TIMEOUT: Duration = Duration::from_millis(2500);

pub struct FallbackEngine {
    id: String,
    chain: Vec<(Arc<dyn SttEngine>, Duration)>,
}

impl FallbackEngine {
    /// `chain` must be non-empty; the first engine is the primary.
    pub fn new(chain: Vec<(Arc<dyn SttEngine>, Duration)>) -> Result<Self> {
        if chain.is_empty() {
            return Err(SttError::InvalidInput("fallback chain is empty".into()));
        }
        let id = chain.iter().map(|(e, _)| e.id()).collect::<Vec<_>>().join(">");
        Ok(Self { id, chain })
    }
}

#[async_trait]
impl SttEngine for FallbackEngine {
    fn id(&self) -> &str {
        &self.id
    }

    fn supports_streaming(&self) -> bool {
        self.chain.first().is_some_and(|(e, _)| e.supports_streaming())
    }

    async fn prewarm(&self) -> Result<()> {
        // Warm only the primary; warming a local fallback would cost RAM for nothing.
        match self.chain.first() {
            Some((e, _)) => e.prewarm().await,
            None => Ok(()),
        }
    }

    fn release(&self) {
        for (e, _) in &self.chain {
            e.release();
        }
    }

    async fn transcribe(&self, audio: &[f32], opts: &SttOptions) -> Result<Transcript> {
        let mut last = SttError::InvalidInput("fallback chain is empty".into());
        for (engine, timeout) in &self.chain {
            match tokio::time::timeout(*timeout, engine.transcribe(audio, opts)).await {
                Ok(Ok(t)) => return Ok(t),
                Ok(Err(e)) => {
                    tracing::warn!(engine = engine.id(), error = %e, "stt engine failed, falling back");
                    last = e;
                }
                Err(_) => {
                    tracing::warn!(engine = engine.id(), ?timeout, "stt engine timed out, falling back");
                    last = SttError::Timeout(*timeout);
                }
            }
        }
        Err(SttError::AllFailed(Box::new(last)))
    }

    fn stream(&self, opts: &SttOptions) -> Option<Box<dyn SttStream>> {
        self.chain.first().and_then(|(e, _)| e.stream(opts))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake {
        id: &'static str,
        delay: Duration,
        fail: bool,
    }

    #[async_trait]
    impl SttEngine for Fake {
        fn id(&self) -> &str {
            self.id
        }
        async fn transcribe(&self, _: &[f32], _: &SttOptions) -> Result<Transcript> {
            tokio::time::sleep(self.delay).await;
            if self.fail {
                return Err(SttError::Network("boom".into()));
            }
            Ok(Transcript { text: self.id.into(), engine_id: self.id.into(), language: None })
        }
    }

    fn fake(id: &'static str, ms: u64, fail: bool) -> Arc<dyn SttEngine> {
        Arc::new(Fake { id, delay: Duration::from_millis(ms), fail })
    }

    #[tokio::test]
    async fn falls_through_on_error_and_timeout() {
        let chain = FallbackEngine::new(vec![
            (fake("err", 0, true), Duration::from_millis(100)),
            (fake("slow", 500, false), Duration::from_millis(50)),
            (fake("ok", 0, false), Duration::from_millis(100)),
        ])
        .unwrap();
        assert_eq!(chain.id(), "err>slow>ok");
        let t = chain.transcribe(&[0.0], &SttOptions::default()).await.unwrap();
        assert_eq!(t.engine_id, "ok");
    }

    #[tokio::test]
    async fn all_failed() {
        let chain = FallbackEngine::new(vec![(fake("err", 0, true), Duration::from_millis(100))]).unwrap();
        assert!(matches!(chain.transcribe(&[0.0], &SttOptions::default()).await, Err(SttError::AllFailed(_))));
    }
}
