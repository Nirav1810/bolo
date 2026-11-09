# 02: Glossary renames (session → dictation, bolo-bench → bolo)

**What to build:** Code and docs use the glossary's words:
- The engine talks about dictations (start, stop and cancel a dictation; the dictation state).
- The CLI is called `bolo`, and the benchmark subcommands stay inside it.

This is a pure prefactor with no behaviour change.

**Blocked by:** 01

**Status:** ready-for-agent

- [x] The engine's public API, its events and the internal per-dictation type are renamed from session to dictation.
- [x] The CLI crate and binary are renamed to `bolo`, and every subcommand still works (`bolo dictate`, `bolo replay`, `bolo run`, …).
- [x] README, BENCH, ARCHITECTURE and CI use the new names.
- [x] `cargo clippy -D warnings` and `cargo test` are green, and `bolo replay` on a few clips gives the same results as before.

## Comments

**2026-11-09: implemented on `refactor/glossary-renames`**
- Engine API: `start/stop/cancel_session` → `start/stop/cancel_dictation`, `SessionState` → `DictationState` (also the `EngineEvents::on_state` argument), and the worker's `Session` struct and field → `Dictation`/`dictation`. `BoloError::Busy` now reads "a dictation is already running". bolo-stt doc comments follow.
- CLI: `crates/bolo-bench` → `crates/bolo`, and the package and binary are now `bolo`. All subcommands are unchanged (`bolo --help` lists models, download, sysinfo, transcribe, record, dictate, cleanup-eval, replay, mic-test and run).
- Docs: README, BENCH, ARCHITECTURE, testdata/README and `.env.example` updated. CI needed no change because it builds the whole workspace. The historical text in ADR-007 (DECISIONS.md) and ADR-0013 still says `bolo-bench` on purpose.
- Verified: `cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings` and `cargo test --workspace` are green. `bolo replay` (local:nemotron-stream, all 40 clips) gives transcripts and per-clip WER identical to `bolo-bench replay` before the rename: en 17.2%, hinglish 31.2%. p50 262 → 289 ms is run-to-run timing noise.
