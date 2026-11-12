# 04: Idle RAM budget

**What to build:** the original spec's idle budget. With settings closed and the model unloaded, `bolo-desktop.exe` is < 60 MB and about 0% CPU.

**Blocked by:** none

**Status:** done

- [x] Closing the settings window, or finishing onboarding, destroys its webview instead of hiding it. The next open builds it again on the right tab. Tray and command paths that assumed a hidden window still work.
- [x] Measure with a script (`scripts/idle_ram.py` or PowerShell), 60 s after the model unloads: private bytes and working set of `bolo-desktop.exe` and of its `msedgewebview2.exe` children, plus CPU. Record before and after in BENCH.md.
- [x] If `bolo-desktop.exe` is still over 60 MB, find out what's holding the memory (allocator, ONNX arenas, the warm mic) and fix it, or record why.
- [x] Clippy and tests are clean.

## Outcome

- **The window.** The `main` window is no longer declared in `tauri.conf.json`. `show_window` builds it with `WebviewWindowBuilder` on `index.html#<page>` when it doesn't exist, and otherwise sets the hash and shows it as before. Closing it is no longer prevented, so the window and its webview are destroyed. The pill still only hides. Onboarding's Finish calls `close()` instead of `hide()` (capability `core:window:allow-close` replaces `allow-hide`).
- **Audit of the paths that open it.** The tray (Settings, History, Set up Bolo, Debug), the second launch (single-instance), first-run onboarding and the restart after an update (About, with `Updated { from }`) all go through `show_window`, which now creates the window when it's missing. The tray's "Update Bolo" installs directly and opens no window. Every Rust event is a broadcast `app.emit`, which is harmless with no window. The pages fetch their state on load: `settings_get` (including running downloads), `update_view` (including `Updated`/`Available`) and `status`. Nothing depends only on pushed events.
- **Fixed on the way.**
  - The Apps tab's "last app in front" listened for focus loss on a window labelled `settings`, which never existed. It now listens on `main`.
  - A window destroyed mid-capture of a new push-to-talk key would have left Bolo without its key. On `Destroyed`, the hook is put back if a capture was in progress and Bolo isn't paused. It isn't put back otherwise, so Quit's unhook stays done.
- **Measuring.** `scripts/idle_ram.ps1` sums private bytes and working set for the app and its `msedgewebview2.exe` descendants, and samples CPU. A `measure` cargo feature with `BOLO_MEASURE=1` runs a build beside an installed Bolo without touching it. See BENCH.md §21 for the numbers and the exact commands.
- **Numbers (BENCH.md §21).** `bolo-desktop.exe` is 9–28 MB private and 0.00% CPU with the model unloaded, under the 60 MB budget, so no allocator or arena work was needed. The WebView2 tree goes from 196 MB (installed 0.3.0) or 269 MB (installed Nightly), where the hidden settings webview stays alive, to 169–185 MB after closing settings, and about 158 MB if it was never opened. What's left is the pill's WebView2, mostly the GPU process (73–98 MB). A `--disable-gpu` run cut the tree to 99 MB. That's a possible follow-up, but it needs a person to check the transparent pill under software compositing.
- **Not exercised by hand:** clicking the tray to reopen each page after a close, and onboarding's Finish. The reopen path is the same code as the startup open, which was run on `settings` and `history`. These clicks belong in the M4 manual checklist (ticket 07).
