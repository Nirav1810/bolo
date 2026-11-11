# M3 report: the Windows desktop app

M3 turns the engine into an app: hold Caps Lock in any Windows app, speak, let go, and the text is pasted where the cursor is. This report covers what shipped, the numbers, and what's still owed. The plan is `.scratch/m3-desktop/` (spec and 15 tickets), with ADR-0017 (Caps Lock push-to-talk) and ADR-0018 (clipboard insertion and the polish swap).

**Measured on 2026-11-11** on the development laptop (16 GB RAM, Realtek mic array), `main` after PR #41.

## What shipped

| ticket | what | PR |
|---|---|---|
| 01 | Tauri v2 + Svelte 5 app shell (`apps/desktop`, crate `bolo-desktop`). One engine in-process, the tray, a single instance, and the `desktop (windows-latest)` CI job | #28 |
| 02 | `bolo-store`: versioned `settings.toml` with migration and bad-file backup, SQLite history (text only, 30-day prune), and API keys in Windows Credential Manager | #29 |
| 03 | HTML output from the rules pipeline and from polish (`Formatted::html`), for Word, Outlook and Gmail | #30 |
| 04 | Designed UI, reviewed and approved: the pill, settings, onboarding and the tray. The tokens live in `src/theme.css` | #35 |
| 05 | Caps Lock push-to-talk (ADR-0017): a `WH_KEYBOARD_LL` hook and a pure hold/tap state machine. A tap under 150 ms is replayed as Caps Lock; a modifier with Caps passes through; Esc cancels | #31 |
| 06 | Clipboard insertion and the polish swap (ADR-0018): save and restore the clipboard, keep pastes out of Win+V history, a UIPI notice for admin windows, and a Shift+Left swap when untouched | #32 |
| 07 | App rules (`app_rules.toml`): Plain, Markdown, HTML or code mode per app, polish off in editors and terminals, user overrides, and history recording | #33 |
| 08 | The overlay pill: non-activating and click-through, with a live waveform and partials, plus polishing, notice and error states | #36 |
| 09 | Settings and history window. Every change is saved and applied live; model downloads; keys masked and testable | #38 |
| 10 | First-run onboarding: mic check (Audio enhancements warning), recommended model download with resume or a Deepgram key, and a practice dictation | #40 |
| 11 | Per-user NSIS installer in the release pipeline, smoke-tested, with an uninstaller that asks about models and history | #41 |
| 12 | Script fallback: Hinglish written in eight other Indic scripts is romanized | #34 |
| 13 | `bolo run`'s finish-only latency explained: the machine, not the code (BENCH.md §20) | #39 |
| 15 | (added during M3, at the user's request) An always-on idle button. Clicking it starts a hands-free dictation with Cancel and Stop. It's draggable and hides in fullscreen apps | #37 |

## Latency

**Key-up → text in the app.** The app logs `key_up_to_paste_ms` (from push-to-talk release to the Ctrl+V being sent) next to the engine's `total_ms`. The live checks so far give four dictations into Notepad: **291, 371, 399 and 465 ms** (median 385 ms). That isn't the 20-dictation p50/p95 the spec asks for: dictation needs a person speaking, so the measurement is owed by the human (see the checklist). The early numbers already show a gap. The engine's own release → text on the same machine is about 180 ms p50 (`bolo replay`, M2 report). That leaves roughly 100–250 ms between the engine's final and the paste, unaccounted for so far. The 20-run measurement should split it with the logged `total_ms`.

**Engine only.** These are unchanged from M2: `local:nemotron-stream` in Auto mode reads 182 ms p50 release → text in real-time `bolo replay`, and `deepgram-stream` reads about 290 ms. `bolo run`'s finish-only clock reads 226–235 ms on today's machine for every build back to `v0.1.0` (BENCH.md §20).

## Resources while idle

Measured with the app in the tray and the settings window hidden. The model was unloaded (30 s idle unload) and sampled over 60 s.

| process | working set | private | CPU |
|---|---|---|---|
| `bolo-desktop.exe`, model unloaded | 32 MB | 10 MB | 0.00% |
| `bolo-desktop.exe`, model loaded (seen right after a dictation) | about 830 MB | | |
| WebView2 (7 processes: the pill and the hidden main window) | 420 MB | 194 MB | |

The app itself is near zero. WebView2 is most of the idle memory. The main window's webview stays alive while hidden. Destroying it on close and building it again on open would cut that, at the cost of a slower first open (gap 3).

## Installer

`Bolo_<version>_x64-setup.exe` is **9.75 MiB** (sherpa-onnx shared), under the 15 MB budget. It needs no admin rights, carries no model, uses the WebView2 bootstrapper, and is unsigned. In LZMA terms, onnxruntime is about 4.3 MiB of it, sherpa-onnx 1.3 MiB and the app 4.2 MiB. The static sherpa build wasn't measured, because it doesn't link with the local MSVC 14.43. The release workflow installs and uninstalls it silently before publishing.

## Polish swap

The swap and its skip reasons are logged and recorded in history (`swapped`, `touched`, `other-window`, `too-late`, `multiline`, `unchanged`, `blocked`). No polished dictations have been made in the app yet, so **the swap rate is owed** from the manual checklist. Enable polish with an Anthropic key in Settings › Engines and keys, then read the History page's captions.

## Manual checklist (human)

For each app: hold Caps Lock, say a sentence, and release. Then say a two-item list ("first milk, second eggs"). Note what arrived and how.

- [ ] Notepad: plain text. **Do 20 dictations here.** Then run `grep -o "key_up_to_paste_ms=[0-9.]*" %LOCALAPPDATA%\Bolo\data\logs\desktop.log` for the p50/p95.
- [ ] Word: lists arrive as real lists (HTML).
- [ ] Gmail in Chrome: HTML (title rule).
- [ ] Slack: Markdown.
- [ ] VS Code: code mode, with no capital letter, no trailing period and no polish.
- [ ] Windows Terminal: the same as VS Code.
- [ ] An elevated window (for example Notepad run as administrator): the text stays on the clipboard and the pill says so.
- [ ] The polish swap, with polish on: (a) don't touch anything, and the text is replaced; (b) type a key right after the paste, and it's skipped as `touched`.
- [ ] Caps Lock: a quick tap toggles caps, Shift+Caps is plain Caps Lock, and Esc while talking cancels.
- [ ] Settings: change the key to F13 or Right Ctrl and dictate; turn Start with Windows on and off; save, test and remove a key.
- [ ] Onboarding on a clean profile: rename `%APPDATA%\Bolo\settings.toml` away, start Bolo, and walk all four steps. Quit at step 3 and restart.
- [ ] Installer on a clean Windows profile: SmartScreen "More info → Run anyway", first run, then uninstall, answering the models prompt.
- [ ] Still open from earlier tickets: the ticket 05 physical-keyboard and LED check, ticket 08's speech/Alt+Tab/second-monitor checks, and ticket 15's mouse checks.

## Known gaps

1. **Owed by the human:** the checklist above. That includes the 20-dictation latency and the swap rate, plus items carried from M1 and M2: the M1 ticket 05 speech check, the M1 ticket 07 clean-profile zip run, the M2 ticket 02 reference sign-off and the M2 ticket 08 live polish try.
2. **Key-up → paste looks 100–250 ms slower than the engine's release → text** in the four samples so far. It isn't investigated yet.
3. **WebView2 holds about 190 MB private while idle,** because the hidden settings window's webview is kept alive.
4. **The first dictation after 30 s idle waits about 2.4 s** while the model reloads. That's the idle-unload trade-off (ADR-009); the warm mic still keeps the first word.
5. **Clipboard formats other than text, HTML, images and files are lost** by a dictation (ADR-0018, documented).
6. **The multi-line polish swap is off everywhere.** No app has been checked by hand and marked `multiline_swap_safe`.
7. **The installer is unsigned and the app doesn't update itself.** SmartScreen warns on first run. The README explains it.
8. **Microphone choice:** Bolo uses the Windows default input; the settings window doesn't pick a device.
9. **Unchanged from M2:** one speaker and one mic in the bench set; rules gaps by design (ADR-0015).

## What M4 starts with

M4's scope comes from the milestone plan and is planned after sign-off; Android and `bolo-ffi` are M5 (ADR-0013, ARCHITECTURE.md). From M3, these carry over:

- Gaps 2 and 3, if the human measurements confirm them: the paste latency, and dropping the hidden webview.
- The manual checklist results. Any app that misbehaves becomes an app-rules change.
- The desktop release now ships two assets per tag, the CLI zip and the installer. The release flow is unchanged otherwise (ADR-0013).

M3 is waiting for sign-off. M4 doesn't start before it.
