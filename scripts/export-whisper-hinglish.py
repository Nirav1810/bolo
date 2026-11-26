#!/usr/bin/env python3
"""Export Oriserve/Whisper-Hindi2Hinglish-Swift to sherpa-onnx's Whisper layout (int8).

Steps (scripts/export-whisper-hinglish.md has the details):
  1. Download the HF checkpoint at the revision pinned in models/manifest.json and check
     its `source_sha256`.
  2. Convert it to an OpenAI-whisper `.pt` with the repo's `convert_hf2openai.json`
     key mapping (the model card's `save_model` logic).
  3. Run sherpa-onnx's `scripts/whisper/export-onnx.py` (pinned to the sherpa-onnx
     release the app links) patched to load that local `.pt`. It writes the fp32 and
     int8 encoder/decoder and the multilingual `base-tokens.txt`.
  4. Pack the int8 files, LICENSE and NOTICE into a reproducible `.tar.bz2` plus
     `.sha256`, ready for the `models-whisper-base-hinglish-v1` GitHub release.

Usage: python -I scripts/export-whisper-hinglish.py [--work DIR]
The default work dir is .cache/whisper-hinglish. It holds the venv, pip cache, downloads
and outputs, so point it at a drive with ~3 GB free. Steps whose outputs exist are skipped.
"""
import argparse, hashlib, io, json, os, pathlib, subprocess, sys, tarfile, urllib.request

ROOT = pathlib.Path(__file__).resolve().parent.parent
MODEL_ID = "whisper-base-hinglish"
SHERPA_VERSION = "2.0.1"  # keep in sync with the `sherpa-onnx` pin in Cargo.toml
SHERPA_COMMIT = "af2d8d81dbfa61c4cea4c568da78f3982edca005"  # tag v2.0.1
EXPORT_SCRIPT_SHA256 = "5e0ddabfac8330ee9aecaba4c1b4286d1cd9f88df4cd30e54706b011bf976e45"
PACKAGE = "sherpa-onnx-whisper-base-hinglish-swift-int8"
OUTPUTS = ["base-encoder.int8.onnx", "base-decoder.int8.onnx", "base-tokens.txt"]
# Small HF files that the conversion reads; the safetensors hash comes from the manifest.
HF_FILES = ["config.json", "convert_hf2openai.json"]
# base-tokens.txt in sherpa's own multilingual Whisper-base package
# (huggingface.co/csukuangfj/sherpa-onnx-whisper-base). The export writes the same file.
TOKENS_SHA256 = "b34b360dbb493e781e479794586d661700670d65564001f23024971d1f2fa126"


def entry():
    manifest = json.loads((ROOT / "models" / "manifest.json").read_text(encoding="utf-8"))
    return next(m for m in manifest["models"] if m["id"] == MODEL_ID)


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        while chunk := f.read(1 << 20):
            h.update(chunk)
    return h.hexdigest()


def fetch(url, out, want_sha256=None):
    if out.exists() and (want_sha256 is None or sha256(out) == want_sha256):
        print(f"{out.name}: present")
        return
    tmp = out.with_name(out.name + ".part")
    req = urllib.request.Request(url, headers={"User-Agent": "bolo-build"})
    with urllib.request.urlopen(req, timeout=120) as r, open(tmp, "wb") as f:
        while chunk := r.read(1 << 20):
            f.write(chunk)
    if want_sha256 and sha256(tmp) != want_sha256:
        tmp.unlink()
        sys.exit(f"{out.name}: sha256 mismatch (want {want_sha256})")
    tmp.replace(out)
    print(f"{out.name}: downloaded")


def run(*args, **kw):
    print("+", " ".join(str(a) for a in args), flush=True)
    subprocess.run([str(a) for a in args], check=True, **kw)


def venv(work):
    """A venv with exactly the pinned requirements (no resolver, no extra packages)."""
    env_dir = work / "venv"
    py = env_dir / ("Scripts/python.exe" if os.name == "nt" else "bin/python")
    reqs = ROOT / "scripts" / "export-whisper-hinglish.requirements.txt"
    stamp = env_dir / ".requirements-sha256"
    if not (py.exists() and stamp.exists() and stamp.read_text() == sha256(reqs)):
        run(sys.executable, "-m", "venv", "--clear", env_dir)
        run(py, "-m", "pip", "install", "--quiet", "--upgrade", "pip==24.3.1")
        run(py, "-m", "pip", "install", "--quiet", "setuptools==84.2.1")
        run(py, "-m", "pip", "install", "--no-deps", "--no-build-isolation", "-r", reqs)
        stamp.write_text(sha256(reqs))
    return py


# Runs inside the venv (`--stage convert`): the model card's convert_hf2openai logic, but
# reading the safetensors and config.json directly instead of through transformers.
def convert(src, out_pt):
    import re
    import torch
    from safetensors.torch import load_file

    cfg = json.loads((src / "config.json").read_text(encoding="utf-8"))
    mapping = json.loads((src / "convert_hf2openai.json").read_text(encoding="utf-8"))
    dims = {
        "n_mels": cfg["num_mel_bins"],
        "n_vocab": cfg["vocab_size"],
        "n_audio_ctx": cfg["max_source_positions"],
        "n_audio_state": cfg["d_model"],
        "n_audio_head": cfg["encoder_attention_heads"],
        "n_audio_layer": cfg["encoder_layers"],
        "n_text_ctx": cfg["max_target_positions"],
        "n_text_state": cfg["d_model"],
        "n_text_head": cfg["decoder_attention_heads"],
        "n_text_layer": cfg["decoder_layers"],
    }
    state, dropped = {}, []
    for key, value in load_file(str(src / "model.safetensors")).items():
        key = key.replace("model.", "")
        new = next((re.sub(p, r, key) for p, r in mapping.items() if re.match(p, key)), None)
        if new is None:
            dropped.append(key)
        else:
            state[new] = value
    print(f"converted {len(state)} tensors; dropped {dropped}")
    torch.save({"dims": dims, "model_state_dict": state}, out_pt)

    import whisper  # the round trip must load with strict key matching

    m = whisper.load_model(str(out_pt), device="cpu")
    print("loads in openai-whisper:", m.dims, "multilingual:", m.is_multilingual)


def patched_export_script(src, dst):
    text = src.read_text(encoding="utf-8")
    old = "    model = load_model(name)\n"
    if text.count(old) != 1:
        sys.exit("export-onnx.py changed upstream; update the patch")
    dst.write_text(text.replace(old, "    model = whisper.load_model(os.environ['BOLO_WHISPER_PT'])\n"), encoding="utf-8")


def check_tokens(out_dir):
    """export-onnx.py writes the vocab in text mode (CRLF on Windows); sherpa's is LF."""
    tokens = out_dir / "base-tokens.txt"
    tokens.write_bytes(tokens.read_bytes().replace(b"\r\n", b"\n"))
    if sha256(tokens) != TOKENS_SHA256:
        sys.exit("base-tokens.txt differs from sherpa's multilingual Whisper-base vocab")
    print("base-tokens.txt: same as sherpa's whisper-base")


def pack(out_dir, dist):
    """Deterministic tar.bz2: sorted names, zero mtimes and owners."""
    dist.mkdir(parents=True, exist_ok=True)
    archive = dist / f"{PACKAGE}.tar.bz2"
    members = OUTPUTS + ["LICENSE", "NOTICE"]
    data = {
        "LICENSE": (ROOT / "LICENSE-APACHE").read_bytes(),
        "NOTICE": NOTICE.encode(),
    }
    with tarfile.open(archive, "w:bz2") as tar:
        for name in sorted(members):
            blob = data.get(name) or (out_dir / name).read_bytes()
            info = tarfile.TarInfo(f"{PACKAGE}/{name}")
            info.size, info.mtime, info.mode = len(blob), 0, 0o644
            tar.addfile(info, io.BytesIO(blob))
    digest = sha256(archive)
    (dist / f"{archive.name}.sha256").write_bytes(f"{digest}  {archive.name}\n".encode())
    (dist / "LICENSE").write_bytes(data["LICENSE"])
    (dist / "NOTICE").write_bytes(data["NOTICE"])
    unpacked = sum(len(data.get(n) or (out_dir / n).read_bytes()) for n in members)
    print(json.dumps({"archive": str(archive), "size_bytes": archive.stat().st_size,
                      "sha256": digest, "root_dir": PACKAGE, "unpacked_size_bytes": unpacked}, indent=1))


NOTICE = """\
Whisper-base Hinglish (sherpa-onnx int8 export)

This package is a format conversion of Whisper-Hindi2Hinglish-Swift by Oriserve
(https://huggingface.co/Oriserve/Whisper-Hindi2Hinglish-Swift, revision
cd0d4d013062d542569135460a79025337df1982), licensed under the Apache License 2.0
(see LICENSE). The weights were converted to ONNX and int8-quantized by the Bolo
project (https://github.com/Nirav1810/bolo, scripts/export-whisper-hinglish.py); they
were not retrained.

Whisper-Hindi2Hinglish-Swift is a fine-tune of openai/whisper-base.
Whisper: Copyright (c) 2022 OpenAI, MIT License
(https://github.com/openai/whisper/blob/main/LICENSE).

base-tokens.txt is Whisper's multilingual tokenizer vocabulary (OpenAI, MIT).
The ONNX export uses scripts/whisper/export-onnx.py from k2-fsa/sherpa-onnx
(Apache-2.0), Copyright 2023 Xiaomi Corp.
"""


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--work", type=pathlib.Path, default=ROOT / ".cache" / "whisper-hinglish")
    ap.add_argument("--stage", choices=["convert"], help=argparse.SUPPRESS)
    args = ap.parse_args()
    work = args.work.resolve()
    src, out_dir = work / "hf", work / "onnx"

    if args.stage == "convert":
        convert(src, work / "whisper-base-hinglish.pt")
        return

    for d in (src, out_dir, work / "tmp"):
        d.mkdir(parents=True, exist_ok=True)
    up = entry()["upstream"]
    base = f"https://huggingface.co/{up['repo']}/resolve/{up['revision']}"
    fetch(f"{base}/{up['source_file']}", src / up["source_file"], up["source_sha256"])
    for name in HF_FILES:
        fetch(f"{base}/{name}", src / name)

    # Keep pip, temp files and any model cache inside the work dir.
    env = dict(os.environ, PIP_CACHE_DIR=str(work / "pip-cache"), TMP=str(work / "tmp"),
               TEMP=str(work / "tmp"), TMPDIR=str(work / "tmp"), XDG_CACHE_HOME=str(work / "xdg-cache"))
    os.environ.update(env)
    py = venv(work)

    pt = work / "whisper-base-hinglish.pt"
    if not pt.exists():
        run(py, "-I", "-X", "utf8", __file__, "--work", work, "--stage", "convert", env=env)

    if not all((out_dir / n).exists() for n in OUTPUTS):
        upstream = work / "export-onnx.upstream.py"
        fetch(f"https://raw.githubusercontent.com/k2-fsa/sherpa-onnx/{SHERPA_COMMIT}/scripts/whisper/export-onnx.py",
              upstream, EXPORT_SCRIPT_SHA256)
        script = work / "export-onnx.py"
        patched_export_script(upstream, script)
        # `--model base` names the outputs base-*.onnx / base-tokens.txt (the multilingual
        # vocab) and sets n_mels = 80; the weights come from BOLO_WHISPER_PT.
        run(py, "-I", "-X", "utf8", script, "--model", "base", cwd=out_dir, env=dict(env, BOLO_WHISPER_PT=str(pt)))

    check_tokens(out_dir)
    pack(out_dir, work / "dist")


if __name__ == "__main__":
    main()
