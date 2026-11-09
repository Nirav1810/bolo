//! Ordered fallback chain, e.g. groq → deepgram → local. Each engine gets its own
//! timeout (default 2.5 s for cloud); on error or timeout the next engine is tried.
//!
//! A streaming primary streams as usual. Its stream also keeps the dictation's audio, so
//! if the primary fails at release (a dropped connection, say), the rest of the chain
//! transcribes that audio in one go.

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

    fn per_stream_language(&self) -> bool {
        self.chain.first().is_some_and(|(e, _)| e.per_stream_language())
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
        run_chain(&self.chain, audio, opts).await
    }

    fn stream(&self, opts: &SttOptions) -> Option<Box<dyn SttStream>> {
        let (primary, _) = self.chain.first()?;
        let stream = primary.stream(opts)?;
        if self.chain.len() == 1 {
            return Some(stream);
        }
        Some(Box::new(FallbackStream {
            primary: stream,
            primary_id: primary.id().to_owned(),
            audio: Vec::new(),
            rest: self.chain[1..].to_vec(),
            opts: opts.clone(),
        }))
    }
}

async fn run_chain(chain: &[(Arc<dyn SttEngine>, Duration)], audio: &[f32], opts: &SttOptions) -> Result<Transcript> {
    let mut last = SttError::InvalidInput("fallback chain is empty".into());
    for (engine, timeout) in chain {
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

/// The primary's stream, plus the audio so far for the rest of the chain.
struct FallbackStream {
    primary: Box<dyn SttStream>,
    primary_id: String,
    audio: Vec<f32>,
    rest: Vec<(Arc<dyn SttEngine>, Duration)>,
    opts: SttOptions,
}

impl SttStream for FallbackStream {
    fn accept(&mut self, samples: &[f32]) {
        self.audio.extend_from_slice(samples);
        self.primary.accept(samples);
    }

    fn partial(&mut self) -> Option<String> {
        self.primary.partial()
    }

    fn finish(self: Box<Self>) -> Result<Transcript> {
        let Self { primary, primary_id, audio, rest, opts } = *self;
        match primary.finish() {
            Ok(t) => Ok(t),
            Err(e) => {
                tracing::warn!(engine = %primary_id, error = %e, "stt stream failed, falling back");
                // `finish` is sync and may be called from inside a runtime, so the chain
                // runs on a runtime of its own on a scoped thread.
                std::thread::scope(|scope| {
                    scope
                        .spawn(|| {
                            tokio::runtime::Builder::new_current_thread()
                                .enable_all()
                                .build()
                                .map_err(|e| SttError::Model(e.to_string()))?
                                .block_on(run_chain(&rest, &audio, &opts))
                        })
                        .join()
                        .unwrap_or_else(|_| Err(SttError::Model("fallback thread panicked".into())))
                })
            }
        }
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
