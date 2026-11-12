# 04: Idle RAM budget

**What to build:** the original spec's idle budget. With settings closed and the model unloaded, `bolo-desktop.exe` is < 60 MB and about 0% CPU.

**Blocked by:** none

**Status:** ready-for-agent

- [ ] Closing the settings window, or finishing onboarding, destroys its webview instead of hiding it. The next open builds it again on the right tab. Tray and command paths that assumed a hidden window still work.
- [ ] Measure with a script (`scripts/idle_ram.py` or PowerShell), 60 s after the model unloads: private bytes and working set of `bolo-desktop.exe` and of its `msedgewebview2.exe` children, plus CPU. Record before and after in BENCH.md.
- [ ] If `bolo-desktop.exe` is still over 60 MB, find out what's holding the memory (allocator, ONNX arenas, the warm mic) and fix it, or record why.
- [ ] Clippy and tests are clean.
