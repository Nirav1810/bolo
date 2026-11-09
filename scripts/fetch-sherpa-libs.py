#!/usr/bin/env python3
"""Fetch the prebuilt sherpa-onnx archive for this host, verify its SHA-256 against the
GitHub release `digest`, and unpack its lib/ folder to .cache/sherpa-lib, which
.cargo/config.toml passes to sherpa-onnx-sys as SHERPA_ONNX_LIB_DIR.

Unpacking outside target/ keeps CI caches from restoring a build-script result whose
extracted libraries were pruned.

Usage: python -I scripts/fetch-sherpa-libs.py [archive-name ...]
Default archive: the one for this host (Windows x64 shared MT Release).
Uses the GitHub API asset route, which keeps working when github.com/.../download 504s.
"""
import hashlib, json, os, pathlib, shutil, sys, tarfile, urllib.request

VERSION = "2.0.1"  # keep in sync with the `sherpa-onnx` pin in Cargo.toml
REPO = "k2-fsa/sherpa-onnx"
DEFAULTS = {
    "win32": f"sherpa-onnx-v{VERSION}-win-x64-shared-MT-Release-lib.tar.bz2",
    "linux": f"sherpa-onnx-v{VERSION}-linux-x64-shared-lib.tar.bz2",
}
CACHE = pathlib.Path(__file__).resolve().parent.parent / ".cache"
DEST = CACHE / "sherpa-archives"
LIB_DIR = CACHE / "sherpa-lib"


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
    unpack(DEST / names[0])


def unpack(archive):
    """Extract <stem>/lib/* into LIB_DIR (replacing what was there)."""
    if LIB_DIR.exists():
        shutil.rmtree(LIB_DIR)
    LIB_DIR.mkdir(parents=True)
    with tarfile.open(archive, "r:bz2") as tar:
        for m in tar.getmembers():
            parts = pathlib.PurePosixPath(m.name).parts
            if len(parts) >= 3 and parts[1] == "lib" and m.isfile() and ".." not in parts:
                src = tar.extractfile(m)
                (LIB_DIR / pathlib.PurePosixPath(*parts[2:])).parent.mkdir(parents=True, exist_ok=True)
                with open(LIB_DIR / pathlib.PurePosixPath(*parts[2:]), "wb") as f:
                    shutil.copyfileobj(src, f)
    print(f"unpacked {sum(1 for _ in LIB_DIR.rglob('*') if _.is_file())} files to {LIB_DIR}")


if __name__ == "__main__":
    main(sys.argv[1:] or [DEFAULTS.get(sys.platform) or sys.exit(f"no default archive for {sys.platform}")])
