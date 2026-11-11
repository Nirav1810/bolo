# 11: NSIS installer in the release pipeline

**What to build:** A per-user NSIS installer for the desktop app, built and published by the existing release pipeline next to the CLI zip, with its size measured against the 15 MB budget.

**Blocked by:** 01

**Status:** ready-for-human (built; the clean-profile install is owed)

- [x] Tauri bundler: NSIS, `installMode: currentUser`, the WebView2 bootstrapper, sherpa-onnx DLLs as resources next to the exe. No model in the installer.
- [x] The uninstaller removes the app. It asks before deleting models and history; settings and keys stay unless the user ticks the box.
- [x] `scripts/release.py package` builds `Bolo_<version>_x64-setup.exe` and its `.sha256` (LF). `release.yml` uploads both. Script tests cover the new names.
- [x] Size reported for sherpa shared vs static (if static links on CI's MSVC), with the onnxruntime share. If it's over 15 MB, report the size; don't cut features to fit.
- [x] Unsigned. The SmartScreen warning is documented in the README install steps.
- [ ] Manual: install on a clean Windows profile, first run, uninstall.

## Comments

**2026-11-11 (agent).**
- `tauri.conf.json` bundles NSIS in `currentUser` mode with the WebView2 download bootstrapper. The four sherpa-onnx DLLs go in as resources next to `bolo-desktop.exe`. No model is bundled.
- `windows/hooks.nsh` runs only on a real uninstall, not an update:
  - it removes the "Start with Windows" Run value;
  - it asks before deleting `%LOCALAPPDATA%\Bolo` (models, history, logs); a silent uninstall answers No;
  - only with Tauri's "Delete the application data" ticked does it delete `%APPDATA%\Bolo` (settings) and the `<provider>.bolo` Credential Manager entries.
- `release.py package` copies `target/release/bundle/nsis/Bolo_<version>_x64-setup.exe` into `dist/` with an LF `.sha256`, and fails if it's missing. `release.yml` builds it with `npx tauri build --bundles nsis`, installs and uninstalls it silently, and publishes both files.
- The npm `@tauri-apps/api` is pinned to 2.11.1 to match the `tauri` crate (2.11.6); the CLI refuses to bundle across a minor-version mismatch.

**Size (sherpa shared, the default): 9.75 MiB**, under the 15 MB budget. LZMA shares:

| file | raw | in the installer |
|---|---|---|
| onnxruntime.dll | 17.0 MiB | about 4.3 MiB (44%) |
| sherpa-onnx-c-api.dll | 4.1 MiB | about 1.2 MiB |
| bolo-desktop.exe | 13.3 MiB | about 4.2 MiB |
| the rest | | about 0.1 MiB |

Sherpa static wasn't measured: the static libs don't link with this machine's MSVC 14.43 (ADR-003 follow-up). It would fold the same onnxruntime and sherpa code into the exe, so expect about the same compressed size.

A silent install and uninstall into a temp folder worked here: all files landed, the uninstall removed them, and the models were kept.

**Manual check owed (human):** install on a clean Windows profile, including the SmartScreen prompt; do the first run; then uninstall interactively, answering the models prompt and the app-data checkbox.
