# M4: Windows polish and release pipeline

**Why:** the original plan's M4 is "Windows polish + release pipeline: onboarding, history, per-app tone/format, autostart, auto-updater, signed installer. ✅ Fresh-VM install → dictate in < 2 minutes." Onboarding, history, autostart and the auto-updater already shipped in M3 and the updates work (`v0.4.0`). M4 finishes the rest of the list and the desktop items from the original spec that were never built: per-app tone and format, snippets, a microphone picker, the idle-RAM budget, update settings and Authenticode signing.

The user signed off M3 on 2026-11-12. The M3 manual checklist is still owed and carries into this milestone's report.

## User-facing behaviour

- **Apps tab** (replaces Polish → Per app). It's one table of apps, listing the bundled ones (Gmail, VS Code, Slack and the rest) plus any app you add. For each app you set **Polish** (on/off/default), **Tone** (default, neutral, casual or formal) and **Format** (default, plain, Markdown or HTML). **Add app** offers the window that was in front most recently, or you can type an exe name. Your row wins over the bundled rules, and resetting a row brings back the bundled values. ADR-0020.
- **Snippets** (in the Dictionary tab, renamed "Dictionary & snippets"). Each snippet is a trigger phrase and the text it expands to. Say the trigger and Bolo inserts the saved text in its place. Matching ignores case and punctuation and is whole-word only. The expansion goes in exactly as written, line breaks included, and polish doesn't rewrite it.
- **Microphone** (General). Pick the input device, or leave it on the Windows default. If the chosen mic is unplugged, Bolo falls back to the default and the pill says so once. Plugging it back in uses it again. The list follows devices being plugged in and out.
- **Idle RAM.** Closing settings, History or onboarding destroys that window's webview. With every window closed and the model unloaded, `bolo-desktop.exe` stays **< 60 MB** private bytes and about 0% CPU. The pill's WebView2 processes are measured and reported separately: the always-on idle button (M3 ticket 15) needs them.
- **Updates** (About). A switch for automatic update checks, on by default. Checks run at launch and every 24 h, as the original spec says (they run every 6 h today). **Check for updates** still works when the switch is off.
- **Signed installer.** Releases and nightlies are Authenticode-signed through SignPath Foundation once the project is approved. Until then the pipeline is in place, switched off, and the README keeps the SmartScreen step. ADR-0021.

## Out of scope

- Android (M5, M6).
- The local LLM (Qwen) polish provider.
- A toggle or double-tap hotkey mode. Hands-free dictation is the idle button's click (M3 ticket 15).
- The `KEYEVENTF_UNICODE` typing fallback for apps that block paste. It becomes a ticket if the manual checks find such an app.
- Investigating key-up → paste latency (M3 gap 2). It waits for the human measurements in the M3 checklist.

## Tickets

1. `01-apps-tab`: per-app polish, tone and format; settings v2 migration; ADR-0020.
2. `02-snippets`: trigger phrases and their expansions in the engine and the Dictionary tab.
3. `03-mic-picker`: choose an input device, fall back to the default, follow hot-plug.
4. `04-idle-ram`: destroy webviews on close, and measure the idle budget.
5. `05-update-settings`: the automatic-check switch and a 24 h interval.
6. `06-authenticode-signing`: SignPath signing in `release.yml` and `nightly.yml`, off until approved; ADR-0021. The human applies to SignPath.
7. `07-m4-report-and-v0.5.0`: the fresh-VM acceptance run, the M4 report and `v0.5.0`. This is the milestone gate.

Tickets 01–06 have no blockers on each other. 01 and 02 both touch `settings.rs` and the Dictionary/Polish pages, so they land one after the other. 07 needs the rest.
