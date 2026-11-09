# 06: Deepgram streaming engine

**What to build:** A streaming Deepgram engine over WebSocket, selectable as its own engine spec next to the batch one.
- Audio streams as 16 kHz PCM16 during the dictation, and finalize is sent at release.
- Cloud text therefore arrives shortly after release, instead of after a full upload.
- The engine is benchmarked with `bolo replay`.

**Blocked by:** 02

**Status:** done

- [x] Implements the STT stream interface: partials during speech, a final after finish. Supports keyterm/dictionary and `language`, including `multi`.
- [x] Uses the existing rustls/ring TLS setup. The licence of any new crate is checked and allowed by cargo-deny.
- [x] The API key is never logged. Connection errors surface as engine errors and work with the fallback chain.
- [x] A unit test covers message handling using recorded or scripted server messages (no network in CI).
- [x] `bolo replay` with the Deepgram streaming spec has been run on the 40-clip set. WER and release→text p50/p95 are recorded in docs/BENCH.md next to batch Deepgram.

## Comments

- 2026-11-09: Implemented as the `deepgram-stream` engine spec (`bolo_stt::cloud::DeepgramStreamEngine`, tokio-tungstenite on rustls/ring with the platform verifier; MIT, cargo-deny clean). A failed stream falls back to the rest of the chain on the buffered audio (`FallbackEngine::stream`). Tests: unit tests for message handling, plus `crates/bolo-stt/tests/deepgram_stream.rs` against a scripted local WebSocket server. Replay results in docs/BENCH.md §8: streaming 299/370 ms p50/p95 vs batch 1306/1907 ms; WER en 9.2% / hinglish 21.0% (batch 7.6% / 22.8%).
