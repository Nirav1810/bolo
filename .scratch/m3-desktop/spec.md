# Spec: M3, the Windows desktop app

**Status:** ready-for-agent

This spec builds on `GLOSSARY.md`, ADR-0017 (Caps Lock push-to-talk) and ADR-0018 (clipboard insertion and the polish swap). It also builds on ADR-009 (warm mic) and ADR-007 (model location) in `docs/DECISIONS.md`, and ADR-0016 (polish in the engine). The scope comes from "What M3 starts with" in `docs/M2-REPORT.md`. The decisions were agreed with the user on 2026-11-10.

## Problem Statement

Bolo works only as a CLI. To dictate, a user has to open a terminal, run `bolo dictate`, then copy the text into the app they're writing in. The point of the product is to talk in any app and have the text appear there, and none of that exists yet:

- no global hotkey;
- no insertion into other apps;
- no on-screen feedback;
- no settings, history or stored keys;
- no installer.

The engine already emits everything a host app needs: `on_level`, `on_partial`, `on_final` and `on_polished`, plus the warm mic and idle model unload.

## Solution

A Windows desktop app in `apps/desktop`, built with Tauri v2 + Svelte 5 + Vite + Tailwind, that hosts the engine in-process.

- **Hold Caps Lock to talk** in any app (ADR-0017). Release, and the text is pasted where the cursor is. A quick tap still toggles Caps Lock, and Esc during a dictation cancels it.
- **An overlay pill** shows that Bolo is listening. It shows a live waveform and the partial text, and it never takes focus.
- **Insertion through the clipboard** (ADR-0018). HTML goes to targets that accept it. The user's clipboard is restored afterwards and kept out of clipboard history.
- **The polish swap.** When polish arrives and the user hasn't touched the keyboard or mouse in the same window, the instant text is selected back and replaced.
- **Target context.** The foreground app chooses Plain, Markdown or HTML, `code_mode`, and whether polish runs. Polish runs everywhere except code editors and terminals, and the user can override this per app.
- **`bolo-store`:**
  - a versioned settings TOML;
  - SQLite history: text only, kept 30 days, can be turned off;
  - API keys in Windows Credential Manager;
  - the user dictionary, which feeds the engine and polish.
- **A designed UI**, with mockups the user reviews before building: the overlay pill, the settings window, onboarding and the tray.
- **Onboarding.** A mic check that detects the ADR-009 dead time, the RAM-tier model recommendation with a resumable download, and a first dictation to try.
- **An NSIS installer.** Per-user install with no admin rights, no bundled model, measured against the 15 MB budget, unsigned, and published next to the CLI zip.
- **An M3 report and `v0.3.0`** (prerelease), followed by a stop for sign-off.

## User Stories

1. As a user, I want to hold Caps Lock, speak and let go, and see my words typed into whatever app I'm in, so that I can dictate anywhere.
2. As a user, I want a quick tap of Caps Lock to still toggle caps, so that I don't lose the key.
3. As a user, I want Shift+Caps Lock (or Caps Lock with any modifier) to behave as plain Caps Lock, so that I always have a way to type in capitals.
4. As a user, I want Esc during a dictation to cancel it, with nothing inserted, so that I can abandon a bad take.
5. As a user, I want to choose a different push-to-talk key in settings (Right Ctrl, Right Alt, F13–F24), so that I can keep Caps Lock if I rely on it.
6. As a user, I want the first word I say kept even when I start speaking the moment I press the key, so that nothing is clipped.
7. As a user, I want a small pill near the bottom of the screen that shows Bolo is listening, with a waveform, so that I know the mic hears me.
8. As a user, I want the pill to show the words as they're recognized, so that I can trust what will be typed.
9. As a user, I want the pill never to steal focus or block clicks, so that dictating doesn't disturb the app I'm typing in.
10. As a user, I want the text inserted within a blink of releasing the key, so that dictation feels instant.
11. As a user, I want my clipboard left as it was after Bolo pastes, so that dictation doesn't clobber what I copied.
12. As a user, I want Bolo's pastes kept out of Windows clipboard history (Win+V), so that history isn't flooded with dictations.
13. As a user writing in Word, Outlook or Gmail, I want lists to arrive as real formatted lists, so that I don't have to reformat.
14. As a user writing in Slack, Notion or Obsidian, I want Markdown, so that lists and line breaks render.
15. As a developer in VS Code or a terminal, I want no auto-capitals, no trailing period and no polish, so that dictated code and commands drop in cleanly.
16. As a user with polish on, I want the polished text to replace the instant text only if I haven't typed or clicked since, so that polish never overwrites my edits.
17. As a user, I want to turn polish on or off per app, so that I control where my words go to Claude.
18. As a user typing into an admin window that Bolo can't paste into, I want the text left on the clipboard and a short notice, so that I don't lose the dictation.
19. As a user, I want my API keys stored in Windows Credential Manager, never in a file, so that they stay safe.
20. As a user, I want a history of my recent dictations that I can search and copy from, so that I can recover text I lost.
21. As a privacy-minded user, I want history to keep text only, prune itself after 30 days, and be easy to switch off or clear, so that I control what is kept.
22. As a user, I want a personal dictionary of names and terms that Bolo and polish respect, so that my vocabulary comes out right.
23. As a user, I want to pick the engine, the language mode (Auto, English, Hinglish) and the Hinglish script in settings, so that Bolo fits how I speak.
24. As a new user, I want onboarding to check my mic and tell me when Windows Audio enhancements clip my first words, so that dictation works from the start.
25. As a new user, I want Bolo to recommend a model for my machine and download it with progress and resume, so that setup is one click.
26. As a new user, I want to try a dictation inside onboarding, so that I know it works before I rely on it.
27. As a user, I want a tray icon with pause, settings, history and quit, so that Bolo stays out of the way.
28. As a user, I want Bolo to start with Windows if I choose, so that it's always ready.
29. As a user, I want an installer that needs no admin rights and downloads quickly, so that installing is painless.
30. As a user, I want Bolo to use almost no CPU or memory while idle, so that it can run all day.
31. As the maintainer, I want the hotkey state machine, insertion decisions, app rules and store covered by tests that need no desktop session, so that regressions show up in CI.
32. As the maintainer, I want an M3 report and a `v0.3.0` prerelease, so that I can sign off M3 before M4 starts.

## Implementation Decisions

- **App shell.**
  - `apps/desktop` is a Tauri v2 app. Its Rust side, `apps/desktop/src-tauri`, is a workspace member but not a default member, so `cargo test` on Linux CI is unaffected.
  - A new CI job, `desktop (windows-latest)`, builds the frontend, then runs clippy and tests on the app crate. It becomes a required check.
  - The app hosts one `Engine`. Its `EngineEvents` implementation forwards to an app controller in Rust. The controller owns insertion, the polish swap and history, and it emits Tauri events to the webviews. The webviews never drive dictation logic; they render state.
  - The app runs as a single instance, with a tray icon.
  - The version is read from the workspace `Cargo.toml` (ADR-0013).
- **Push-to-talk (ADR-0017).**
  - A `WH_KEYBOARD_LL` hook runs on its own thread with a message loop. The hook procedure only classifies the key and posts it, and it returns within microseconds.
  - The hold/tap logic is a pure state machine with an injected clock, so it can be unit tested. Its inputs are key events and timer ticks. Its outputs are Start, Stop, Cancel, ReplayTap and PassThrough.
  - Caps Lock is swallowed. If it is released before 150 ms, a real Caps Lock tap is replayed with `SendInput`, tagged through `dwExtraInfo` so the hook ignores it. A hold of 150 ms or more starts a dictation.
  - The warm mic's pre-roll covers the 150 ms. The ticket verifies that the pre-roll is at least 300 ms.
  - Key auto-repeat is ignored. Caps Lock with any modifier held passes through untouched.
  - Esc is swallowed only while a dictation is active, and it cancels.
  - Other keys can be chosen in settings: Right Ctrl, Right Alt, F13–F24.
  - Hands-free (toggle) mode is out of scope.
- **Insertion (ADR-0018).**
  - Save the clipboard: `CF_UNICODETEXT`, `CF_HTML`, `CF_DIBV5` and `CF_HDROP`. Other formats are lost; this is documented.
  - Write the text, plus HTML for HTML targets. Set `ExcludeClipboardContentFromMonitorProcessing` and `CanIncludeInClipboardHistory` = 0.
  - Send Ctrl+V with `SendInput`. Apps listed in the app rules get Shift+Insert instead.
  - Restore the saved clipboard after 300 ms, but only if the clipboard sequence number is still ours.
  - If `SendInput` is blocked because the target is elevated (UIPI), leave the text on the clipboard and show a notice in the pill.
- **Polish swap (ADR-0018).**
  - After an insert, record the foreground window and the inserted text's caret length: grapheme clusters, with `\r\n` counted as one.
  - The keyboard hook, plus a `WH_MOUSE_LL` hook that is installed only while a swap is pending, sets a "touched" flag on any key or click.
  - When `on_polished` arrives, swap only if all of these hold:
    - the flag is clear;
    - the same window is in front;
    - the polish arrived within 5 s of the insert;
    - the inserted text is a single line, or the app is listed as multi-line-safe in the app rules.
  - The swap sends Shift+Left once per caret unit, then pastes the polished text the same way as an insert.
  - Each swap or skip, with its reason, is logged and recorded in history.
- **Target context.**
  - The foreground window's process image name (lower-case exe), plus the window title for browsers, is mapped through a bundled app-rules table (TOML in the app crate) to a `TargetContext`: the format, `code_mode`, polish on/off, the paste chord and multi-line swap safety.
  - Code editors and terminals get `code_mode` and polish off. Word, Outlook and Gmail (in a browser, by title) get HTML. Slack, Notion, Obsidian and Discord get Markdown. Everything else gets Plain.
  - User overrides per exe live in settings and win over the table.
  - The matching is a pure function, tested with tables.
- **HTML output.**
  - `bolo_format::rules::format` gains an HTML rendering next to plain and Markdown. It covers paragraphs, line breaks and `<ol>` lists, with all text HTML-escaped.
  - The engine fills `FormattedOutput::html` for `OutputFormat::Html`, and for polished output too. The polished text is rendered with the same renderer, never inserted as raw model HTML.
- **`bolo-store`.**
  - Settings TOML at `%APPDATA%\Bolo\settings.toml`, with `version = 1` and a migration hook. Unknown fields are kept, a bad file is backed up and replaced with defaults, and settings map onto `EngineConfig`.
  - History in SQLite (`rusqlite`, bundled) at `%LOCALAPPDATA%\Bolo\history.sqlite`. Each row stores the time, the app, the language, the engine, the final text, the polished text and the swap outcome, and the timings. Rows are pruned at startup and daily, after 30 days by default. History can be turned off. "Clear history" deletes the rows and vacuums.
  - Secrets go through the `keyring` crate (Windows Credential Manager, service `bolo`). Keys are loaded into `ApiKeys` and never serialized. Tests use keyring's mock store.
  - The dictionary is a list in settings. It feeds `EngineConfig::dictionary` and polish's `dictionary`.
  - The CLI keeps reading its keys from environment variables. Wiring it to the store is out of scope.
- **Designed UI.**
  - Before building any screen, the agent produces design mockups as a published HTML artifact using the `ui-ux-pro-max` and `emil-design-eng` skills: the overlay pill states (listening, processing, polishing, notice, error), the settings window, the onboarding steps and the tray menu.
  - The user reviews them, and approved design tokens go into `apps/desktop` as Tailwind theme values.
  - Light and dark mode follow Windows.
- **Overlay pill.**
  - A transparent, always-on-top, non-activating (`WS_EX_NOACTIVATE`), click-through (`WS_EX_TRANSPARENT`) window, bottom-centre on the monitor that holds the foreground window.
  - The waveform is driven by `on_level`, throttled to the display's frame rate. Partials appear as one line, truncated from the left.
  - The pill hides 600 ms after insert, or after the swap when one is pending.
- **Onboarding and models.**
  - First run opens onboarding: welcome → mic check → model → try it.
  - The mic check opens the device and measures leading digital silence the way `mic-test` does. Over 200 ms, it explains how to turn off Audio enhancements and links to the Windows sound settings.
  - The model step uses `Manifest::recommend` with the machine's RAM and the Hinglish preference, then `bolo_models::Downloader` with progress, resume and SHA-256 checks. Cloud engines can be picked instead, with a key.
  - "Try it" runs a real dictation into a text box in the onboarding window.
- **Installer.**
  - Tauri's NSIS bundler in `currentUser` install mode, with the WebView2 bootstrapper. The sherpa-onnx DLLs are bundled next to the exe.
  - "Start with Windows" uses the Tauri autostart plugin and is off by default.
  - `scripts/release.py package` builds the installer as well as the CLI zip. `release.yml` uploads it with its `.sha256`.
  - The size is measured and reported against 15 MB in both sherpa link modes (the ADR-003 follow-up). If it goes over, report it and don't cut features to fit.
  - The installer is unsigned. The updater is out of scope.
- **Gap fixes carried from M2.**
  - A general script fallback: Roman-mode output that contains any non-Latin script other than Devanagari or Urdu (for example the Telugu in hinglish/09) is transliterated with a generic Indic → ISO 15919 → simple-Roman table, or the dictation falls back to the other Auto pass when one exists.
  - Investigate `bolo run`'s Nemotron finish-only latency (285–295 ms vs M1's 158 ms).

## Testing Decisions

- Test external behaviour: the commands a key sequence produces, the target context an exe and title produce, the store's round trips, the HTML a text produces, and the insert/swap decision for a given history of events. Don't test Win32 plumbing.
- **Hotkey:** a state machine test with a fake clock covers:
  - a tap under 150 ms → ReplayTap;
  - a hold → Start, then Stop on release;
  - auto-repeat ignored;
  - Shift+Caps → PassThrough;
  - Esc while active → Cancel;
  - Esc while idle → PassThrough;
  - an injected (tagged) key → ignored.
- **Swap decision:** a pure function over (touched, same window, age, line count, app safety) with table tests. Win32 calls sit behind a small `Desktop` trait, so the controller can be tested with a fake that records clipboard writes and the keys sent.
- **App rules:** table tests, including user overrides and the browser-title rules.
- **Store:**
  - settings round trip and migration from an old version;
  - a bad file falls back to defaults with a backup;
  - history insert, search, prune and clear;
  - secrets through keyring's mock store.
- **HTML:** golden cases next to the rules' golden TOML, plus a property test: the HTML has no unescaped `<` from the input, and its text content equals the plain rendering, modulo whitespace and list numbering.
- **Manual (the human):** dictation into Notepad, Word, Chrome (Gmail), Slack, VS Code and Windows Terminal; the polish swap; an admin window; and the installer on a clean profile. These go in the M3 report checklist.
- **Measured, not in CI:**
  - key-up → text-in-Notepad latency p50/p95 over 20 dictations (instrumented: key-up time to the paste sent, plus the engine's release → text);
  - idle RAM and CPU with the model unloaded;
  - installer size.

## Out of Scope

- The auto-updater and signed update feeds (later milestone).
- Code signing.
- A hands-free (toggle) mode and double-tap gestures.
- Reading the caret position to place the pill near the cursor.
- Insertion through UI Automation or direct typing (`SendInput` of characters).
- The CLI using `bolo-store` for its settings or keys.
- macOS and Linux desktop builds.
- Audio in history.

## Further Notes

- Tickets are in `issues/`. Tickets 01, 02, 03, 04, 12 and 13 have no blockers. Ticket 04 (design) stops for the user's review before any UI ticket (08, 09, 10) starts. Ticket 14 is the milestone gate.
- Adding `desktop (windows-latest)` to the ruleset's required checks is a GitHub settings change made in ticket 01 (ruleset id 26847972).
