# Exporting `whisper-base-hinglish`

`scripts/export-whisper-hinglish.py` turns [Oriserve/Whisper-Hindi2Hinglish-Swift](https://huggingface.co/Oriserve/Whisper-Hindi2Hinglish-Swift) (Apache-2.0, a Whisper-base fine-tune that writes Roman Hinglish) into the files sherpa-onnx's offline Whisper recognizer loads. The result is hosted on this repo's `models-whisper-base-hinglish-v1` release, and `models/manifest.json` downloads it from there.

```sh
python -I scripts/export-whisper-hinglish.py --work E:/somewhere/whisper-hinglish
```

The work dir (default `.cache/whisper-hinglish`) holds everything: the venv, pip cache, temp files, downloads and outputs, about 3 GB in all. Steps whose outputs exist are skipped, so a rerun only re-packs.

## Steps

1. **Download** `model.safetensors`, `config.json` and `convert_hf2openai.json` at the revision pinned in the manifest's `upstream`. The safetensors must match `upstream.source_sha256`.
2. **Venv.** Python 3.12 with exactly `scripts/export-whisper-hinglish.requirements.txt`, installed with `--no-deps` (torch 2.5.1 CPU, openai-whisper 20250625, onnx 1.17.0, onnxruntime 1.20.1). openai-whisper is an sdist, so it builds against the pinned setuptools with no build isolation.
3. **HF → OpenAI `.pt`.** The model card's `save_model` logic: strip `model.`, rename each key with the regexes in `convert_hf2openai.json`, and take the dims from `config.json`. It reads the safetensors directly rather than through transformers. The only key the mapping skips would be `proj_out.weight`, which is tied to the token embedding and isn't stored. The `.pt` is then loaded back with `whisper.load_model`, which matches keys strictly.
4. **ONNX + int8.** sherpa-onnx's `scripts/whisper/export-onnx.py`, fetched at the commit of tag `v2.0.1` (the runtime we link) and checked by SHA-256, with one line patched: `load_model(name)` becomes `whisper.load_model($BOLO_WHISPER_PT)`. It runs as `--model base`, which names the files `base-*` and picks 80 mel bins. It exports fp32 encoder/decoder (opset 17), writes `base-tokens.txt` from openai-whisper's multilingual vocab, and quantizes both with `quantize_dynamic` (MatMul, QInt8).
5. **Tokens.** `base-tokens.txt` is rewritten with LF endings (the export writes it in text mode, so CRLF on Windows) and must then equal the `base-tokens.txt` in sherpa's own `sherpa-onnx-whisper-base` package.
6. **Pack.** `dist/sherpa-onnx-whisper-base-hinglish-swift-int8.tar.bz2` holds the two int8 models, the tokens, `LICENSE` (Apache-2.0) and `NOTICE`, with sorted names and zeroed mtimes and owners. A rerun gives the same bytes. Next to it are the `.sha256` (LF), `LICENSE` and `NOTICE` for the release, and the script prints the size, hash and unpacked size for the manifest.

## Publishing a new version

The ONNX bytes depend on the torch and onnxruntime versions, so a re-export on another machine can hash differently. Publish it as a new release (`models-whisper-base-hinglish-v2`), never by replacing the assets of an existing one: installed copies and the manifest pin the old hash.

```sh
gh release create models-whisper-base-hinglish-v2 --prerelease --latest=false \
  --title "Model: whisper-base-hinglish v2" --notes-file <notes> dist/*
gh release view models-whisper-base-hinglish-v2 --json isLatest   # must be false
```

The tag doesn't start with `v`, so `release.yml` (`tags: ["v*"]`), `release.py`, git-cliff and the nightly's `git describe --match 'v[0-9]*'` all ignore it. `--prerelease --latest=false` keeps it off `releases/latest`, which is the Beta channel's update feed.

Then update the manifest entry's `download` (URL, `size_bytes`, `sha256`), `unpacked_size_bytes` and `verified_at`, and check it with `bolo download whisper-base-hinglish` and `bolo transcribe`.
