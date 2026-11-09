# 06: Deepgram streaming engine

**What to build:** A streaming Deepgram engine over WebSocket, selectable as its own engine spec next to the batch one.
- Audio streams as 16 kHz PCM16 during the dictation, and finalize is sent at release.
- Cloud text therefore arrives shortly after release, instead of after a full upload.
- The engine is benchmarked with `bolo replay`.

**Blocked by:** 02

**Status:** ready-for-agent

- [ ] Implements the STT stream interface: partials during speech, a final after finish. Supports keyterm/dictionary and `language`, including `multi`.
- [ ] Uses the existing rustls/ring TLS setup. The licence of any new crate is checked and allowed by cargo-deny.
- [ ] The API key is never logged. Connection errors surface as engine errors and work with the fallback chain.
- [ ] A unit test covers message handling using recorded or scripted server messages (no network in CI).
- [ ] `bolo replay` with the Deepgram streaming spec has been run on the 40-clip set. WER and release→text p50/p95 are recorded in docs/BENCH.md next to batch Deepgram.
