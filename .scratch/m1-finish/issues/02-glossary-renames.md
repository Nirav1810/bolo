# 02: Glossary renames (session → dictation, bolo-bench → bolo)

**What to build:** Code and docs use the glossary's words:
- The engine talks about dictations (start, stop and cancel a dictation; the dictation state).
- The CLI is called `bolo`, and the benchmark subcommands stay inside it.

This is a pure prefactor with no behaviour change.

**Blocked by:** 01

**Status:** ready-for-agent

- [ ] The engine's public API, its events and the internal per-dictation type are renamed from session to dictation.
- [ ] The CLI crate and binary are renamed to `bolo`, and every subcommand still works (`bolo dictate`, `bolo replay`, `bolo run`, …).
- [ ] README, BENCH, ARCHITECTURE and CI use the new names.
- [ ] `cargo clippy -D warnings` and `cargo test` are green, and `bolo replay` on a few clips gives the same results as before.
