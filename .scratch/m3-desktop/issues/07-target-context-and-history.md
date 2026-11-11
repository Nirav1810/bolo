# 07: Target app rules, per-app polish, history recording

**What to build:** Map the foreground app to a `TargetContext` and a polish decision through a bundled app-rules table plus user overrides, and record every dictation in history.

**Blocked by:** 02, 06

**Status:** done

- [x] Foreground detection: `GetForegroundWindow` → process image name (lower-case exe), plus the window title for browsers (chrome, msedge, firefox, brave, arc).
- [x] `app_rules.toml`, bundled. Each rule matches an exe (and an optional title pattern) and sets the format, `code_mode`, polish, the paste chord and `multiline_swap_safe`. Defaults:
  - code editors and terminals (code, cursor, idea64, devenv, windowsterminal, wezterm, alacritty, conhost/cmd, powershell, pwsh) → `code_mode`, polish off;
  - winword, outlook, olk, Gmail by title → HTML;
  - slack, notion, obsidian, discord → Markdown;
  - everything else → Plain, with polish following the global setting.
- [x] User overrides per exe from settings beat the table. The pure `resolve(exe, title, rules, overrides)` is table-tested.
- [x] The engine gets the right `TargetContext` and polish config per dictation (`update_config` only when the polish decision changes).
- [x] History: each dictation writes the final row at insert and updates it with the polished text and the swap outcome. No write when history is off.

**Done (agent):**

- `apps/desktop/src-tauri/app_rules.toml` is the bundled table. `src/rules.rs` holds the pure `resolve` (plus `polish_default`, the global polish switch) and `plan`, which turns the result into the engine's `TargetContext` and `Polish`, and the inserter's `Target`.
- Only browsers send their window title to the engine.
- A rule's `name` (for example "Gmail" or "Slack") becomes polish's `app`. Without one, the exe stem is used.
- The bundled table adds `openconsole.exe`, `code - insiders.exe` and `mintty.exe` (Git Bash, pasting with Shift+Insert).
- No app is marked `multiline_swap_safe` yet. ADR-0018 says to mark one only after checking it by hand.
- Overrides match the exe ignoring case, with or without `.exe`. An override turns polish on even where the table or the global switch has it off. Polish still needs the Anthropic key.
- **Deviation:** `update_config` drops the STT engine, which reloads the model on the next dictation. So the per-dictation polish change uses a new `Engine::set_polish`, which swaps polish alone and keeps the model loaded. `Session` sends it only when the plan's polish differs from the last one it sent.
- History is handled by `src/journal.rs`:
  - it writes the inserted rendering (Markdown for Markdown targets, plain text otherwise), even when the paste was blocked;
  - when polish arrives, it updates the row with `swapped`, the skip reason or `failed`;
  - it prunes at startup and then daily.
- The desktop app now reads its engine config from `bolo-store`: settings plus the keys in Credential Manager. `BOLO_*` variables are applied on top for development.
