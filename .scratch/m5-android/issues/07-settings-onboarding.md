# 07: The settings app and onboarding

**What to build:** the launcher activity: first-run onboarding and the settings pages, per the approved mockups.

**Blocked by:** 03, 04, 05

**Status:** ready-for-agent

- [ ] Onboarding: welcome → enable Bolo (`Settings.ACTION_INPUT_METHOD_SETTINGS`, detected on return via `InputMethodManager.enabledInputMethodList`) → pick it (`showInputMethodPicker`) → mic permission → language → model download (recommended by RAM; can be skipped with a cloud key) → test box. Progress is saved (`onboarding_step`, as desktop does), so leaving and coming back resumes.
- [ ] Settings pages: General (language, engine chain, polish, tone), Models (ticket 05), Cloud keys (masked, test-key button, stored via the Keystore `SecretStore`), Dictionary & snippets, Apps (package rows; "add app" lists the installed launchable apps), History (search, copy, delete, clear) and About (version, licences including model attributions, logs share).
- [ ] Edits save through `Bolo.save_settings`, and the IME picks them up at its next dictation without restarting.
- [ ] Compose UI tests for onboarding step resumption and the settings forms (JVM where possible, instrumented for the IME-enable detection).
