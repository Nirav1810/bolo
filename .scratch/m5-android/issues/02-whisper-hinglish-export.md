# 02: Export Whisper-base Hinglish for low-tier phones

**What to build:** turn `whisper-base-hinglish` from `needs-export` into an `available` sherpa model hosted by this repo, and measure it.

**Blocked by:** none

**Status:** ready-for-agent

- [ ] `scripts/export-whisper-hinglish.py`: download `Oriserve/Whisper-Hindi2Hinglish-Swift` at the pinned revision (manifest `upstream`), check `source_sha256`, convert HF → OpenAI `.pt` (the repo's `convert_hf2openai` logic), then run sherpa-onnx's `scripts/whisper/export-onnx.py`, patched to load a local `.pt`, then quantize to int8. Reuse the multilingual `base-tokens.txt`. Pin every Python dependency, run in a venv under `.cache/`, and write a short README for the steps.
- [ ] Check that sherpa-onnx 2.0.1 loads the export (`bolo transcribe -m whisper-base-hinglish` on a hinglish clip) and that it emits Roman Hinglish.
- [ ] Host it: a GitHub release `models-whisper-base-hinglish-v1` in this repo (not a version tag, so `release.py` and the update feeds ignore it), with the `.tar.bz2`, its `.sha256`, the Apache-2.0 license and a NOTICE crediting Oriserve and OpenAI Whisper (MIT).
- [ ] Manifest: `status: available`, `download` (URL, size, sha256), `unpacked_size_bytes`, `verified_at`. `recommends_by_ram` now gets `whisper-base-hinglish` for a 4 GB phone wanting Hinglish; update the test.
- [ ] Bench (`--speed 50`): raw and formatted WER on the 40-clip set for whisper-base-hinglish against Moonshine-tiny (en) and Nemotron (hinglish), plus model RAM and RTF on this PC. Add a BENCH.md section.
- [ ] `bolo-models` tests stay green, and `cargo deny` passes.
