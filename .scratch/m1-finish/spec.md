# Spec: Finish M1 (Auto language mode, trunk guardrails, first release)

**Status:** ready-for-agent

Grounded in `GLOSSARY.md`, ADR-0013 (single version, squash-merged trunk, releases) and ADR-0014 (Auto language mode). Predecessor decisions: `docs/DECISIONS.md` ADR-001…012, especially ADR-012 (the LLM never blocks the instant path).

## Problem Statement

Today a Bolo user must say in advance whether a dictation is English or Hinglish. If they pick wrong, the text is unusable (English decoded as Hindi is 67.6% WER), and Nemotron's built-in automatic detection is wrong often enough to double English errors (8.0% → 17.2%). Someone who switches between English and Hinglish through the day has to keep changing a setting, which is exactly the friction Bolo exists to remove.

Separately, the project has no release mechanics yet: there is no version discipline, no changelog, no protected `main`, and nothing a user can download. M1 also isn't finished. The `dictate` command still uses its own pipeline instead of the live engine, the CLI is still called `bolo-bench`, and the engine's vocabulary ("session") disagrees with the glossary ("dictation").

## Solution

- **Auto language mode by default.** Each dictation is decoded as English and as Hindi at the same time on the one loaded model. At key release Bolo picks one result for the whole dictation, so text appears as fast as with a fixed language. Users can still pin **English** or **Hinglish** in the language mode setting. Ambiguous dictations reuse the previous dictation's language.
- **Trunk guardrails.** `main` changes only through squash-merged PRs with Conventional Commit titles, gated by CI.
- **A release pipeline.** A one-command release PR is followed by a tag that publishes a GitHub Release.
- **The first release.** `v0.1.0` ships the `bolo` CLI for Windows at the end of M1, along with the M1 report.

## User Stories

1. As a bilingual user, I want Bolo to work out whether I spoke English or Hinglish, so that I never have to change a setting between dictations.
2. As a bilingual user, I want each dictation to come out entirely in one language style, so that a paragraph doesn't switch between Devanagari-derived and English spellings halfway through.
3. As an English-only user, I want to set the language mode to English, so that Bolo spends no CPU on a Hindi pass and can never misread me as Hindi.
4. As a Hinglish-only user, I want to set the language mode to Hinglish, so that short English-looking phrases ("ok send it") stay in my Hinglish style.
5. As a user dictating a very short phrase like "ok", I want Bolo to reuse the language of my previous dictation, so that ambiguous words don't flip my output style.
6. As a first-time user with no history, I want ambiguous dictations treated as English, so that the default is predictable.
7. As a user in Auto mode, I want the live preview to show the language that is currently winning, so that the preview resembles what will be inserted.
8. As a user in Auto mode, I want text inserted no slower than with a fixed language (about 160–180 ms after release), so that Auto costs me nothing I can feel.
9. As a Hinglish user, I want English words in my speech written in normal English spelling ("meeting", "confirm"), so that Hinglish output reads naturally (existing loanword behaviour, preserved in Auto).
10. As a user of a cloud engine (Deepgram), I want Auto mode to use the provider's own multilingual mode, so that I get the provider's best mixed-language accuracy without a second paid request.
11. As a user on a low-end Android phone (later milestone), I want to choose my language at onboarding instead of Auto, so that the phone isn't asked to decode twice.
12. As a user trying Bolo from the terminal, I want `bolo dictate` to use the same engine the apps will use (warm mic, Auto mode, VAD segment-ahead decoding), so that what I test is what ships.
13. As a user of `bolo dictate --polish`, I want polish to run after the instant text is already on my clipboard, so that the LLM never delays me (ADR-012).
14. As a user, I want to download a Windows zip, unzip it and run `bolo dictate`, so that I can try Bolo without installing Rust.
15. As a user reading a release, I want release notes grouped into features and fixes, so that I can see what changed.
16. As a user choosing stability, I want stable releases and beta prereleases to be clearly separate, so that I only get untested builds when I opt in.
17. As the maintainer, I want `main` to accept changes only through squash-merged PRs that passed CI, so that every commit on `main` is releasable and bisectable.
18. As the maintainer, I want CI to reject PR titles that aren't Conventional Commits, so that the changelog generator always has clean input.
19. As the maintainer, I want one command that prepares a release PR (version bump everywhere plus regenerated changelog), so that releasing is routine and hard to get wrong.
20. As the maintainer, I want pushing a `vX.Y.Z` tag to build and publish the release automatically, and a `vX.Y.Z-beta.N` tag to publish a prerelease, so that releases are reproducible from CI.
21. As the maintainer, I want the pipeline exercised with a beta tag before `v0.1.0`, so that the first real release isn't also the pipeline's first run.
22. As the maintainer, I want the crates marked not-for-publishing, so that nobody accidentally publishes internal APIs to crates.io.
23. As a contributor, I want the code to use the glossary's words (dictation, not session; `bolo`, not `bolo-bench`), so that code, docs and conversation share one vocabulary.
24. As a contributor, I want the engine's dictation behaviour (start, stop, cancel, language choice, previous-language fallback) covered by tests that run in CI without model downloads, so that regressions are caught on every PR.
25. As the maintainer, I want Auto mode measured on the 40-clip set against fixed bars (English WER ≤ 8.5%, Hinglish WER ≤ 30%, release→text p50 ≤ 180 ms), so that it only becomes the default if it really is as good.
26. As the maintainer, I want a streaming Deepgram engine benchmarked through the same replay harness, so that the cloud option's instant-path latency is known before the apps are built.
27. As the maintainer, I want an M1 report summarising accuracy, latency and what shipped, so that I can confirm M1 before M2 starts.

## Implementation Decisions

- **Glossary renames (prefactor).**
  - The engine's public vocabulary moves from "session" to "dictation": start/stop/cancel dictation, the dictation state enum, and the internal per-dictation struct.
  - The CLI crate and binary move from `bolo-bench` to `bolo`. Benchmark subcommands stay inside it.
  - Docs (README, BENCH, ARCHITECTURE) follow.
- **Language mode.**
  - The engine config's language field carries Auto / English / Hinglish. Hinglish is the existing Hindi-forced decode followed by romanization.
  - Auto behaviour depends on the engine:
    - **Streaming local engines that expose a per-stream language option (Nemotron):** two streams on the one loaded recognizer, one forced `en` and one forced `hi`. Both are fed the same audio during the dictation.
    - **Cloud engines:** their native multilingual mode (Deepgram `language=multi`).
    - **Other local engines:** their own default behaviour. Auto is not offered on low-tier Android (a later milestone).
- **Language decision.**
  - Made once per dictation at release, from the two pass outputs.
  - Signal: the density of Hindi function words in the Hindi pass's Devanagari output. The word list lives with the existing Hindi stoplist in the format layer.
  - Below a confidence margin, the result is "ambiguous" and resolves to the previous dictation's language (English if none).
  - The previous-language memory lives in the engine for the life of the process.
  - Thresholds are tuned on the 40-clip set, and the tuned constants are recorded in the code with the bench numbers that justified them.
- **Live preview in Auto.** Each partial is emitted from whichever pass the same decision rule currently favours (with the same fallback), then romanized as today.
- **Engine test seam.** The engine gains a way to be built with an injected STT engine, alongside the existing injected audio source. CI tests can then drive a full dictation with a scripted fake STT engine and a replayed waveform, with no models and no network. This extends the existing `with_audio_source` seam rather than adding a new public surface elsewhere.
- **`bolo dictate`** is rebuilt on the engine (the same events the apps will consume):
  - Enter starts and stops a dictation; partials are shown live; the final text goes to the clipboard.
  - `--polish` runs the LLM afterwards and replaces the clipboard.
  - A `--lang auto|en|hi` flag defaults to auto.
- **`bolo replay`** gains `--lang` so that Auto can be measured against the shipping bars.
- **Streaming Deepgram engine.**
  - A WebSocket streaming implementation of the STT stream interface, selectable as its own engine spec next to the existing batch Deepgram engine.
  - Audio is sent as 16 kHz PCM16 during the dictation; finalize is sent at release. The API key is never logged.
  - Benchmarked with `bolo replay`.
- **Trunk guardrails.**
  - A CI workflow lints PR titles against Conventional Commits. Allowed types: feat, fix, perf, refactor, docs, test, ci, build, chore, revert. Scopes are optional.
  - Repo settings allow squash merges only, with the PR title as the commit message.
  - A ruleset on `main` requires a PR, requires the CI checks (both rust jobs, deny, PR title), blocks force pushes and deletion, and requires linear history. It requires zero approvals, since there is a single maintainer.
- **Crates.** Every workspace crate is marked `publish = false`.
- **Release mechanics.**
  - The release script takes `X.Y.Z` or `X.Y.Z-beta.N`. It refuses to run off an up-to-date clean `main`.
  - On a release branch, the script:
    - bumps the workspace version (and later the Tauri and Gradle versions);
    - regenerates `CHANGELOG.md` with git-cliff;
    - opens a `chore(release): vX.Y.Z` PR.
  - It supports a dry run that prints the planned changes.
  - git-cliff config groups commits by Conventional Commit type, and `chore(release)` commits are skipped.
  - A tag-triggered workflow builds the Windows x64 `bolo` CLI in release mode and zips it with the sherpa-onnx shared libraries, a README and the licence files. It then publishes a GitHub Release with that version's changelog section as notes. Tags containing `-beta.` are marked as prereleases.
- **Versioning.** One product version (ADR-0013). Pre-1.0 semver: minor for features or breaking changes to settings, patch for fixes.

## Testing Decisions

- **What makes a good test:** it drives the engine through its public API (configure, start dictation, feed audio, stop dictation, observe events) and asserts on what a user would see: final text, chosen language, event order, timings present. It does not reach into worker internals.
- **Primary seam, in CI:** the engine, built with a replayed waveform audio source and a scripted fake STT engine. The fake returns preset English-pass and Hindi-pass text. Cases:
  - English chosen for English text;
  - Hinglish chosen for Hindi function-word-rich text;
  - an ambiguous dictation reuses the previous language;
  - a first ambiguous dictation goes to English;
  - a forced mode never runs the second pass;
  - cancel produces no final;
  - the dictation state sequence is Idle → Listening → Processing → Done → Idle.
- **Accuracy and latency gate, local only** (it needs downloaded models and the maintainer's private clips): `bolo replay` on the 40-clip set with `--lang auto`, `en` and `hi`, against the shipping bars. Results are recorded in `docs/BENCH.md`. This is the acceptance check for making Auto the default.
- **Release script:** a dry run in CI asserts the files it would change and that version strings are consistent across manifests. The real end-to-end check is pushing `v0.1.0-beta.1` and confirming the prerelease and zip appear and the zip runs `bolo --help`.
- **Prior art:**
  - `bolo-format` unit tests (loanwords, LLM guard);
  - the `bolo-core` config test;
  - `bolo replay` as the existing real-time engine harness.

## Out of Scope

- Windows code signing and the Tauri updater keypair (M3 grilling).
- The desktop and Android apps, Play Store, F-Droid and the Android keystore (M3 and M5).
- Polish improvements and the rules engine with golden tests (M2).
- Per-sentence or per-word language switching (rejected in ADR-0014).
- Remembering the previous language across app restarts.
- Publishing crates to crates.io (ADR-0013).
- A nightly channel.

## Further Notes

- Once trunk guardrails land, every later ticket goes through a branch and a squash-merged PR, including this spec's own follow-ups.
- The M1 milestone gate still applies. After `v0.1.0`, the M1 report is presented and work stops until the user confirms M2.
