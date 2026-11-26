# 02: Export Whisper-base Hinglish for low-tier phones

**What to build:** turn `whisper-base-hinglish` from `needs-export` into an `available` sherpa model hosted by this repo, and measure it.

**Blocked by:** none

**Status:** done

- [x] `scripts/export-whisper-hinglish.py`: download `Oriserve/Whisper-Hindi2Hinglish-Swift` at the pinned revision (manifest `upstream`), check `source_sha256`, convert HF → OpenAI `.pt` (the repo's `convert_hf2openai` logic), then run sherpa-onnx's `scripts/whisper/export-onnx.py`, patched to load a local `.pt`, then quantize to int8. Reuse the multilingual `base-tokens.txt`. Pin every Python dependency, run in a venv under `.cache/`, and write a short README for the steps.
- [x] Check that sherpa-onnx 2.0.1 loads the export (`bolo transcribe -m whisper-base-hinglish` on a hinglish clip) and that it emits Roman Hinglish.
- [x] Host it: a GitHub release `models-whisper-base-hinglish-v1` in this repo (not a version tag, so `release.py` and the update feeds ignore it), with the `.tar.bz2`, its `.sha256`, the Apache-2.0 license and a NOTICE crediting Oriserve and OpenAI Whisper (MIT).
- [x] Manifest: `status: available`, `download` (URL, size, sha256), `unpacked_size_bytes`, `verified_at`. `recommends_by_ram` now gets `whisper-base-hinglish` for a 4 GB phone wanting Hinglish; update the test.
- [x] Bench (`--speed 50`): raw and formatted WER on the 40-clip set for whisper-base-hinglish against Moonshine-tiny (en) and Nemotron (hinglish), plus model RAM and RTF on this PC. Add a BENCH.md section.
- [x] `bolo-models` tests stay green, and `cargo deny` passes.

## Outcome

- **Export.** `scripts/export-whisper-hinglish.py` (stdlib only, so it runs as `python -I`) downloads the pinned revision, checks `source_sha256`, and builds a venv from `scripts/export-whisper-hinglish.requirements.txt`. That file pins all 33 packages (torch 2.5.1 CPU, openai-whisper 20250625, onnx 1.17.0, onnxruntime 1.20.1), and they're installed with `--no-deps`. The script converts HF → OpenAI `.pt` with the repo's `convert_hf2openai.json` (245 tensors, none dropped; it loads back in openai-whisper with strict keys). Then it runs sherpa-onnx's `export-onnx.py` at tag `v2.0.1` (SHA-256 checked) with one line patched to load the local `.pt`, and the script's own int8 step (`quantize_dynamic`, MatMul). Steps are in `scripts/export-whisper-hinglish.md`. The work dir defaults to `.cache/whisper-hinglish` and takes `--work`. This run used E:.
- **Deviations.**
  - `base-tokens.txt` came out CRLF on Windows (text mode). The script rewrites it with LF and checks it's byte-identical to the file in sherpa's own `sherpa-onnx-whisper-base`.
  - The model card's conversion goes through transformers. Ours reads the safetensors and `config.json` directly with the same key mapping and dims, which avoids the transformers dependency.
  - Upstream's export CI pins torch 1.13 on Python 3.8. We use torch 2.5.1 on Python 3.12, and the encoder/decoder sizes come within 0.06% of sherpa's own Whisper-base int8 files.
- **sherpa-onnx 2.0.1 loads it** through `bolo transcribe -e local:whisper-base-hinglish`, with no code change. All 40 outputs are ASCII, so Roman Hinglish (e.g. "Mujhe aaj office se jaldi nikalna hai doctor ka appointment hai.").
- **Hosted:** release [`models-whisper-base-hinglish-v1`](https://github.com/Nirav1810/bolo/releases/tag/models-whisper-base-hinglish-v1). It's a prerelease, not Latest (the API's `releases/latest` is still `v0.5.1`). Assets: `sherpa-onnx-whisper-base-hinglish-swift-int8.tar.bz2` (135,755,362 bytes, sha256 `db12583c…5176`; the pack is reproducible), its `.sha256` (LF), `LICENSE` (Apache-2.0) and `NOTICE` (Oriserve, OpenAI Whisper MIT, sherpa's export script). The tag doesn't start with `v`, so `release.yml` (`v*`), `release.py`, git-cliff's `tag_pattern` and the nightly's `git describe --match 'v[0-9]*'` ignore it.
- **Manifest** (`manifest_version` 2026.11.26):
  - `status: available`, with `download`, `unpacked_size_bytes` 160,592,756 and an entry-level `verified_at` 2026-11-26. Serde ignores the unknown field, like the top-level one.
  - `whisper_language` changed from `""` to `en`: it's the model card's setting, scores the same and skips the detection pass.
  - `min_ram_mb` changed from 350 to 800: the measured peak RSS was 419 MB.
  - `bolo download whisper-base-hinglish` into an empty dir fetched it from the release, verified the hash and installed it. `recommends_by_ram` now expects it for 4 GB and 3 GB phones wanting Hinglish.
- **Bench (BENCH.md §22):**
  - Replay at `--speed 50`, WER / fmt WER: whisper-base-hinglish reads en 14.5% / 11.3% and hinglish **26.8% / 30.0%**. Nemotron Auto reads 10.3% / 9.0% and 27.7% / 30.9%. Moonshine-tiny en reads 40.1% / 39.5% at 50×, which is a replay artifact (it drops first words), and 24.0% / 23.4% at real time.
  - `bolo run --runs 3`: whisper p50 645 ms, RTF 0.133, peak RSS 419 MB, load 1.0 s. Moonshine is RTF 0.022 at 132 MB, and Nemotron 750 MB.
  - Real-time release → text for whisper: p50 668 ms, p95 1108 ms.
  - One real-time replay stopped silently after 34 clips (no error, no crash event). The two reruns were clean, and the cause is unknown.
