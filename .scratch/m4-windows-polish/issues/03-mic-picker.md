# 03: Microphone picker

**What to build:** choose the input device, fall back to the default when it's gone, and follow hot-plug.

**Blocked by:** none

**Status:** ready-for-human (the hot-plug check)

- [x] `bolo-store`: `[audio] input_device = "<name>"`. Absent means the Windows default.
- [x] `bolo-audio`: list the input devices, and open one by name, falling back to the default when that name isn't present. Wherever the warm mic re-opens, it re-resolves the device, so an unplugged mic falls back and a re-plugged one is used again.
- [x] Desktop: a `microphones` command, the General tab's Microphone row (Windows default + devices) and the onboarding mic check, which uses the chosen device. A fallback shows a one-time pill notice.
- [x] Unit tests for the name → device resolution, with the device list injected. The hot-plug path gets a manual check in the ticket (Human).
- [x] Clippy, tests and `npm run check` are clean.

## Human: hot-plug check

- [ ] With a USB or Bluetooth headset: pick it in Settings › General › Microphone and dictate (it's used: the Windows mic indicator names Bolo on that device). Unplug it and dictate twice: the first dictation's pill says "Mic unplugged. Using the Windows default" and the text still comes; the second shows no notice. Plug it back in and dictate: it's used again (within about 3 s if the mic was still warm). Unplug it **during** a dictation: that one may come out empty, the next one works on the default.
- [ ] With the Microphone list open, plug or unplug a device, close and reopen the list: it follows.
- [ ] Onboarding's mic check (add `onboarding_step = 1` to `settings.toml` and restart Bolo) names the chosen device.

## Outcome

- **Settings.** `[audio]` is a new, additive table in settings v2 (`AudioSettings`: `input_device: Option<String>` skipped when absent, `#[serde(default)]`, flattened `unknown`), so there's no version bump. `Settings::engine_config*` passes the trimmed name to the new `EngineConfig::input_device` (`None` = default). Round-trip and "absent means default" tests.
- **bolo-audio.** `device::pick(wanted, &names)` is the pure resolution (`Default` / `Device(i)` / `Missing`): exact name first, then case- and space-insensitive. `capture::input_devices()`, `default_input_device()` and `is_available(name)` read the list from cpal each call. `Capture::start(Option<&str>)` resolves against the devices plugged in at that moment; a missing device, or one that won't open, falls back to the default with `fell_back()`. The cpal error callback now sets `failed()` on lasting errors (anything but `Xrun`, `RealtimeDenied` and `DeviceChanged`, which WASAPI uses for a rerouted default stream). `start_default()` is kept for the CLI.
- **Engine.** `AudioFactory` takes the device name, and `AudioSource` grew `device_name`, `fell_back`, `failed` and `wanted_is_back` (default no-ops, so `WavSource` is unchanged). The worker passes `config.input_device` to every open. A failed stream is closed between dictations and replaced at the next `start_dictation` (never wedged); a warm mic on the fallback looks for the chosen device every 3 s and reopens on it, still warm. `Engine::set_input_device` (and `update_config` with a new device) reopens a warm mic once idle without touching the STT model. `EngineEvents::on_mic_fallback(wanted, using)` (default no-op) fires once per unplug: not again until an open lands on the chosen device or the choice changes. Covered by `crates/bolo-core/tests/mic.rs` with a fake device list.
- **Desktop.** `microphones` command (names plus the Windows default's name, off the main thread). General › Basics has a Microphone select: Windows default, the devices, and the saved one marked "(not plugged in)" when it's gone; the list re-reads on focus/pointer-down so it follows hot-plug. `Form.input_device` → `Changes.mic`, which sends `set_input_device` and leaves the model loaded. The controller holds the fallback notice and emits it with the next text (sent at once it would be hidden by Listening, since the mic opens first); a blocked paste's notice goes first and the mic notice waits. Onboarding's `mic_check` uses the chosen device and reports `fell_back`, which the "didn't hear anything" alert explains.
- Device names on this machine (read-only listing, nothing recorded): `Microphone Array (Realtek(R) Audio)` (default) and `External Microphone (Realtek(R) Audio)`; enumeration takes a few ms.
- Screenshots (mocked backend, headless Edge): `mic-default-light.png`, `mic-chosen-light.png`, `mic-chosen-dark.png`, `mic-missing-light.png`.
