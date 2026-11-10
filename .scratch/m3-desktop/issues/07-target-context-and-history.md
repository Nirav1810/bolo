# 07: Target app rules, per-app polish, history recording

**What to build:** Map the foreground app to a `TargetContext` and a polish decision through a bundled app-rules table plus user overrides, and record every dictation in history.

**Blocked by:** 02, 06

**Status:** ready-for-agent

- [ ] Foreground detection: `GetForegroundWindow` → process image name (lower-case exe), plus the window title for browsers (chrome, msedge, firefox, brave, arc).
- [ ] `app_rules.toml`, bundled. Each rule matches an exe (and an optional title pattern) and sets the format, `code_mode`, polish, the paste chord and `multiline_swap_safe`. Defaults:
  - code editors and terminals (code, cursor, idea64, devenv, windowsterminal, wezterm, alacritty, conhost/cmd, powershell, pwsh) → `code_mode`, polish off;
  - winword, outlook, olk, Gmail by title → HTML;
  - slack, notion, obsidian, discord → Markdown;
  - everything else → Plain, with polish following the global setting.
- [ ] User overrides per exe from settings beat the table. The pure `resolve(exe, title, rules, overrides)` is table-tested.
- [ ] The engine gets the right `TargetContext` and polish config per dictation (`update_config` only when the polish decision changes).
- [ ] History: each dictation writes the final row at insert and updates it with the polished text and the swap outcome. No write when history is off.
