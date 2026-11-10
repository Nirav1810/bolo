# 11: NSIS installer in the release pipeline

**What to build:** A per-user NSIS installer for the desktop app, built and published by the existing release pipeline next to the CLI zip, with its size measured against the 15 MB budget.

**Blocked by:** 01

**Status:** ready-for-agent

- [ ] Tauri bundler: NSIS, `installMode: currentUser`, the WebView2 bootstrapper, sherpa-onnx DLLs as resources next to the exe. No model in the installer.
- [ ] The uninstaller removes the app. It asks before deleting models and history; settings and keys stay unless the user ticks the box.
- [ ] `scripts/release.py package` builds `Bolo_<version>_x64-setup.exe` and its `.sha256` (LF). `release.yml` uploads both. Script tests cover the new names.
- [ ] Size reported for sherpa shared vs static (if static links on CI's MSVC), with the onnxruntime share. If it's over 15 MB, report the size; don't cut features to fit.
- [ ] Unsigned. The SmartScreen warning is documented in the README install steps.
- [ ] Manual: install on a clean Windows profile, first run, uninstall.
