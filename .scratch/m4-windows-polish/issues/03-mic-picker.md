# 03: Microphone picker

**What to build:** choose the input device, fall back to the default when it's gone, and follow hot-plug.

**Blocked by:** none

**Status:** ready-for-agent

- [ ] `bolo-store`: `[audio] input_device = "<name>"`. Absent means the Windows default.
- [ ] `bolo-audio`: list the input devices, and open one by name, falling back to the default when that name isn't present. Wherever the warm mic re-opens, it re-resolves the device, so an unplugged mic falls back and a re-plugged one is used again.
- [ ] Desktop: a `microphones` command, the General tab's Microphone row (Windows default + devices) and the onboarding mic check, which uses the chosen device. A fallback shows a one-time pill notice.
- [ ] Unit tests for the name → device resolution, with the device list injected. The hot-plug path gets a manual check in the ticket (Human).
- [ ] Clippy, tests and `npm run check` are clean.
