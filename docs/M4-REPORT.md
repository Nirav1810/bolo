# M4 report: Windows polish and the release pipeline

M4 finishes the original plan's "Windows polish + release pipeline" milestone: onboarding, history, per-app tone and format, autostart, auto-updater and a signed installer, with the goal of a fresh install → first dictation in under 2 minutes. Onboarding, history and autostart shipped in M3. The auto-updater shipped between milestones, in the updates work (`v0.4.0`, ADR-0019). This report covers the rest. The plan is `.scratch/m4-windows-polish/` (spec and 7 tickets), with ADR-0020 (per-app profiles) and ADR-0021 (Authenticode through SignPath).

**Measured on 2026-11-12** on the development laptop (16 GB RAM, Realtek mic array), `main` after PR #65.

## What shipped

| ticket | what | PR |
|---|---|---|
| 01 | **Apps tab** (ADR-0020). Per-app polish, tone and format. The user's `[[apps]]` rows win over the bundled `app_rules.toml` field by field. You can add your own apps, and "Add app" offers the last app you used. Settings go to version 2, with a migration from `[polish.apps]` | #60 |
| 02 | **Snippets.** Say a trigger phrase and the saved text goes in its place. Matching is whole-word and ignores case, punctuation and spoken-vs-digit numbers. The engine expands snippets last (plain, Markdown and HTML), and polish keeps the triggers word for word. "Dictionary & snippets" tab | #62 |
| 03 | **Microphone picker.** `[audio] input_device`. An unplugged mic falls back to the Windows default with a one-time pill notice, and a re-plugged one is used again. A failed stream is replaced at the next dictation. Changing the mic doesn't reload the model | #63 |
| 04 | **Idle RAM.** Closing the settings window destroys its webview, and opening it builds it again on the right page. `scripts/idle_ram.ps1` measures, and a `measure` build feature lets a test build run next to an installed Bolo (BENCH.md §21) | #65 |
| 05 | **Update settings.** "Check for updates automatically" switch (on by default). Checks run at launch and once a day, instead of every 6 h | #64 |
| 06 | **Authenticode signing through SignPath Foundation** (ADR-0021), in `release.yml` and `nightly.yml`. It stays off until the `SIGNPATH_API_TOKEN` secret exists. The updater `.sig` and the `.sha256` are computed over the signed file. `docs/SIGNING.md`, and a code signing policy in the README | #61 |
| updates 04 | `v0.4.0`, the first release that updates itself; both feeds checked | #57, #58 |

## Budgets

| budget (original spec) | measured | |
|---|---|---|
| Desktop idle RAM, model unloaded: < 60 MB | `bolo-desktop.exe` **8.6–9.6 MB private** (32–42 MB working set) with settings closed. The installed Nightly read 27.9 MB after real dictations | ✅ |
| Idle CPU ~0% | 0% for the app, 0.02–0.07% for WebView2 | ✅ |
| Windows installer < 15 MB (no models) | `Bolo-Nightly_0.5.0-nightly.20261112.20_x64-setup.exe` **10.2 MiB** | ✅ |
| Rules + formatter < 2 ms | unchanged from M2. Snippet expansion is a single pass over the words | ✅ |
| Release → text | engine unchanged from M3: 182 ms p50 Auto (`bolo replay`). Key-up → paste is still owed by the human (M3 gap 2) | owed |

The pill's WebView2 is outside the 60 MB budget (6 processes, 157–185 MB private). The always-on idle button (M3 ticket 15) needs it, and the original spec's budget was written before that button existed. Most of it is the WebView2 GPU process: a one-off run with `--disable-gpu` brought WebView2 to 99 MB. That isn't shipped until someone checks how the transparent pill renders without the GPU.

## Fresh install → dictate in < 2 minutes

This is the milestone's acceptance test, and it needs a person and a clean machine: download → SmartScreen (until signing is on) → install → onboarding (mic check, model download or Deepgram key, practice dictation) → first dictation in Notepad. The model download dominates: the recommended local model is a few hundred MB, so on a slow connection the Deepgram path is the one that fits in 2 minutes. **Owed by the human** (checklist).

## Manual checklist (human)

M4:

- [ ] **Acceptance:** on a fresh Windows VM or a clean user profile, time download → first dictation. Under 2 minutes?
- [ ] Apps tab: set Slack to Markdown and Casual, dictate a list in Slack, and check the format. Add an app with "Add app" (last app used) and set its tone.
- [ ] Snippets: add "my address" → a two-line address. Dictate "send it to my address" in Notepad and in Word (HTML). Then try it with polish on.
- [ ] Microphone: pick the external mic and dictate. Unplug it: the next dictation says "Mic unplugged. Using the Windows default". Plug it back in and dictate again.
- [ ] Settings window: close it, then reopen it from the tray on Settings, History and About. Finish onboarding on a clean profile, and check its window closes and Settings opens fine afterwards.
- [ ] About: turn automatic update checks off and back on. **Check now** works either way.
- [ ] Signing: apply to SignPath Foundation (`docs/SIGNING.md`), add the secret and variable, then cut a release and check `Get-AuthenticodeSignature` and SmartScreen.

Carried from M3 (docs/M3-REPORT.md has the details): the per-app checks (Notepad with 20 dictations for key-up → paste p50/p95, Word, Gmail, Outlook, Slack, WhatsApp Web, VS Code, Windows Terminal, an elevated window), the polish swap, Caps Lock, settings and keys, onboarding and the installer on a clean profile, ticket 05's physical keyboard check, ticket 08's speech, Alt+Tab and second-monitor checks, and ticket 15's mouse checks. Also from the updates work: install v0.4.0 and Bolo Nightly by hand, and check that Nightly updates itself.

## Known gaps

1. **Owed by the human:** the checklist above, including the acceptance run and the SignPath application.
2. **Unsigned until SignPath approves.** `bolo-desktop.exe` inside the installer won't be signed even then, because SignPath can't open an NSIS installer. SmartScreen judges the installer itself. The CLI zip isn't signed either.
3. **The pill's WebView2 holds about 160–185 MB private while idle**, mostly its GPU process. `--disable-gpu` is a candidate fix, pending a visual check.
4. **Key-up → paste looks 100–250 ms slower than the engine's release → text** (M3 gap 2). The cause isn't investigated yet, waiting on the human's 20-dictation log.
5. **The microphone is stored by name.** Two identical headsets look the same. A Windows rename of the device falls back to the default.
6. **The update timer uses the monotonic clock,** so time the PC spends asleep may not count toward the 24 h.
7. **Snippet triggers that the rules only change in context aren't matched.** Examples are a trigger containing "actually" (self-correction) or "first" (list detection).
8. **Unchanged from M3:** the first dictation after 30 s idle waits about 2.4 s while the model reloads. Clipboard formats other than text, HTML, images and files are lost. The multi-line polish swap is off everywhere.

## What M5 starts with

M5 is the Android voice keyboard: `bolo-ffi` (UniFFI → Kotlin), the IME, a settings app and a model download by RAM tier, working in WhatsApp, Gmail, Chrome and Notes on a ≥ 6 GB phone and on a 3–4 GB phone (Moonshine or Whisper-base), within the idle budgets and with no battery use while the keyboard is hidden. M6 is the Android release pipeline. From M4, these carry over:

- Settings v2 (`[[apps]]`, `[[snippets]]`, `[audio]`, `[updates]`) is the schema the Android settings app reads and writes through `bolo-ffi`. Snippets and per-app profiles (Android: by package name) come along for free in the engine.
- The DECISIONS.md note on revisiting Opus/FLAC uploads on a mobile connection.

M4 is waiting for sign-off. M5 doesn't start before it.
