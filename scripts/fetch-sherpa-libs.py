#!/usr/bin/env python3
"""Fetch the prebuilt sherpa-onnx archive that sherpa-onnx-sys expects, verify its
SHA-256 against the GitHub release `digest`, and place it in .cache/sherpa-archives.

Usage: python -I scripts/fetch-sherpa-libs.py [archive-name ...]
Default archive: the one for this host (Windows x64 shared MT Release).
Uses the GitHub API asset route, which keeps working when github.com/.../download 504s.
"""
import hashlib, json, os, pathlib, sys, urllib.request

VERSION = "2.0.1"  # keep in sync with the `sherpa-onnx` pin in Cargo.toml
REPO = "k2-fsa/sherpa-onnx"
DEFAULTS = {
    "win32": f"sherpa-onnx-v{VERSION}-win-x64-shared-MT-Release-lib.tar.bz2",
    "linux": f"sherpa-onnx-v{VERSION}-linux-x64-shared-lib.tar.bz2",
}
DEST = pathlib.Path(__file__).resolve().parent.parent / ".cache" / "sherpa-archives"


def get(url, accept="application/vnd.github+json"):
    headers = {"Accept": accept, "User-Agent": "bolo-build"}
    if os.environ.get("GITHUB_TOKEN"):
        headers["Authorization"] = "Bearer " + os.environ["GITHUB_TOKEN"]
    return urllib.request.urlopen(urllib.request.Request(url, headers=headers), timeout=120)


def main(names):
    release = json.load(get(f"https://api.github.com/repos/{REPO}/releases/tags/v{VERSION}"))
    assets = {a["name"]: a for a in release["assets"]}
    DEST.mkdir(parents=True, exist_ok=True)
    for name in names:
        a = assets.get(name)
        if a is None:
            sys.exit(f"{name}: not in release v{VERSION}")
        want = (a.get("digest") or "").removeprefix("sha256:")
        out = DEST / name
        if out.exists() and hashlib.sha256(out.read_bytes()).hexdigest() == want:
            print(f"{name}: already present, sha256 ok")
            continue
        tmp = out.with_suffix(".part")
        h = hashlib.sha256()
        with get(a["url"], "application/octet-stream") as r, open(tmp, "wb") as f:
            while chunk := r.read(1 << 20):
                h.update(chunk)
                f.write(chunk)
        if not want or h.hexdigest() != want:
            tmp.unlink()
            sys.exit(f"{name}: sha256 mismatch (got {h.hexdigest()}, want {want or 'no digest'})")
        tmp.replace(out)
        print(f"{name}: {a['size']} bytes, sha256 ok")


if __name__ == "__main__":
    main(sys.argv[1:] or [DEFAULTS.get(sys.platform) or sys.exit(f"no default archive for {sys.platform}")])
