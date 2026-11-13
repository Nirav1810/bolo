#!/usr/bin/env python3
"""Cut a Bolo release (ADR-0013): one product version, tags on `main`, stable + beta.

The flow, from a clean, up-to-date `main`:

    python -I scripts/release.py 0.1.0-beta.1 --dry-run   # print the planned changes
    python -I scripts/release.py 0.1.0-beta.1             # open the chore(release) PR
    # ...CI goes green and the PR is squash-merged, then on the updated `main`:
    python -I scripts/release.py tag 0.1.0-beta.1          # tag it; the tag publishes

Subcommands (a bare version means `prepare`):
  prepare X.Y.Z[-beta.N] [--dry-run]
      Create release/vX branch, bump the workspace version, regenerate CHANGELOG.md with
      git-cliff, commit `chore(release): vX` and open the PR. --dry-run changes nothing.
  tag X.Y.Z[-beta.N]
      Tag the merged release commit at the tip of `main` and push the tag.
  check [--tag vX] [--on-main]
      Version strings agree across manifests (and equal the tag, which is on `main`).
  notes vX [--output FILE]
      That version's CHANGELOG.md section: the GitHub Release notes.
  package vX
      Zip target/release/bolo.exe with the sherpa-onnx DLLs, README and licences, and
      copy the desktop installer that `npx tauri build` made (apps/desktop). Each gets
      a .sha256.
  feed vX --url-base URL
      Write dist/latest.json, the updater feed (ADR-0019): the installer's URL under
      URL, its minisign signature (the .sig `tauri build` wrote) and the changelog notes.
  sha256 FILE...
      Rewrite FILE.sha256 for each file: after SignPath has Authenticode-signed the
      installer (ADR-0021), its checksum must cover the signed bytes.
  nightly-version --run N
      Print the next nightly's version: next minor after the newest release, plus
      `-nightly.<UTC YYYYMMDD>.N`.
  set-version X.Y.Z-nightly.D.N
      Write a nightly version into Cargo.toml and Cargo.lock for one CI build; not committed.
  publish vX ASSET...
      Create the GitHub Release as Latest; only `-beta.` versions are prereleases.

Needs git, plus git-cliff (`pip install git-cliff`) for prepare and gh for prepare/publish.
"""

from __future__ import annotations

import argparse
import datetime
import difflib
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import sysconfig
import tomllib
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MAIN = "main"
# Explicit, so origin/main is updated even in a tag-only CI checkout.
MAIN_REFSPEC = f"+refs/heads/{MAIN}:refs/remotes/origin/{MAIN}"
VERSION_RE = re.compile(r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-beta\.([1-9]\d*))?$")
# A nightly build's version (ADR-0019): never committed, never a release tag.
NIGHTLY_RE = re.compile(r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)-nightly\.(\d{8})\.([1-9]\d*)$")
# Files a release PR changes. M5 adds the Android Gradle file: bump it in plan() and check
# it in manifest_errors(). The desktop app has no version of its own to bump: Tauri takes
# the bolo-desktop crate's, which is the workspace's (see DESKTOP_MANIFESTS).
RELEASE_FILES = ("Cargo.toml", "Cargo.lock", "CHANGELOG.md")
# JSON manifests of the desktop app that must not carry a `version`: without one, Tauri
# uses the crate version, so the workspace stays the single source (ADR-0013).
DESKTOP_MANIFESTS = ("apps/desktop/src-tauri/tauri.conf.json", "apps/desktop/package.json")


class ReleaseError(Exception):
    pass


# --- versions --------------------------------------------------------------------------


def version_key(version: str) -> tuple[int, int, int, int, int]:
    """Sort key: 0.1.0-beta.1 < 0.1.0-beta.2 < 0.1.0 < 0.1.1-beta.1."""
    m = VERSION_RE.match(version)
    if not m:
        raise ReleaseError(f"not a release version: {version!r} (want X.Y.Z or X.Y.Z-beta.N)")
    major, minor, patch, beta = m.groups()
    stable = beta is None
    return int(major), int(minor), int(patch), int(stable), 0 if stable else int(beta)


def parse_tag(tag: str) -> str:
    """The version in a `vX.Y.Z[-beta.N]` tag, or in a nightly's `vX.Y.Z-nightly.D.N` name."""
    if not tag.startswith("v"):
        raise ReleaseError(f"release tags start with v: {tag!r}")
    if not NIGHTLY_RE.match(tag[1:]):
        version_key(tag[1:])
    return tag[1:]


def nightly_version(tags: list[str], date: datetime.date, run_number: int) -> str:
    """The next minor after the newest release, plus `-nightly.YYYYMMDD.N`: it sorts below
    the release that will ship the same work, so that release overtakes it (ADR-0019)."""
    stables = [version_key(t[1:]) for t in release_tags(tags) if not is_beta(t)]
    major, minor = (stables[-1][0], stables[-1][1] + 1) if stables else (0, 1)
    if run_number < 1:
        raise ReleaseError(f"run number must be positive: {run_number}")
    return f"{major}.{minor}.0-nightly.{date:%Y%m%d}.{run_number}"


def is_beta(version: str) -> bool:
    return "-beta." in version


def is_prerelease(version: str) -> bool:
    """Only old-style `-beta.N` versions. 0.x releases publish as Latest so the Beta feed
    `/releases/latest/download/latest.json` resolves; the app's badge says Beta (ADR-0019)."""
    return is_beta(version)


def release_tags(tags: list[str]) -> list[str]:
    """The tags that name releases, oldest first."""
    found = [t for t in tags if t.startswith("v") and VERSION_RE.match(t[1:])]
    return sorted(found, key=lambda t: version_key(t[1:]))


def version_errors(version: str, tags: list[str]) -> list[str]:
    version_key(version)
    released = release_tags(tags)
    if f"v{version}" in released:
        return [f"v{version} is already tagged"]
    if released and version_key(version) <= version_key(released[-1][1:]):
        return [f"v{version} is not newer than the latest release, {released[-1]}"]
    return []


def folded_betas(tags: list[str], version: str) -> list[str]:
    """Beta tags whose commits belong in a stable section of the changelog.

    A stable release absorbs its betas, so its notes list everything since the previous
    stable release. Betas of an already-released version are folded the same way; betas
    still heading for an unreleased version keep their own sections.
    """
    released = release_tags(tags)
    stables = [version_key(t[1:])[:3] for t in released if not is_beta(t)]
    if not is_beta(version):
        stables.append(version_key(version)[:3])
    return [t for t in released if is_beta(t) and version_key(t[1:])[:3] in stables]


# --- manifests -------------------------------------------------------------------------


def workspace_version(cargo_toml: str) -> str:
    return tomllib.loads(cargo_toml)["workspace"]["package"]["version"]


def bump_cargo_toml(text: str, version: str) -> str:
    """Rewrite `version` under [workspace.package], keeping comments and layout."""
    pattern = re.compile(r'(^\[workspace\.package\]\n(?:(?!\[).*\n)*?version = ")[^"]*(")', re.M)
    new, n = pattern.subn(rf"\g<1>{version}\g<2>", text)
    if n != 1:
        raise ReleaseError("Cargo.toml: no `version` under [workspace.package]")
    return new


def member_names(root: Path = ROOT) -> list[str]:
    members = tomllib.loads((root / "Cargo.toml").read_text("utf-8"))["workspace"]["members"]
    return [tomllib.loads((root / m / "Cargo.toml").read_text("utf-8"))["package"]["name"] for m in members]


LOCK_ENTRY = re.compile(r'^\[\[package\]\]\nname = "(?P<name>[^"]+)"\nversion = "(?P<version>[^"]+)"\n(?P<next>.*)$', re.M)


def lock_versions(lock: str, names: list[str]) -> dict[str, str]:
    """Versions of the workspace crates in Cargo.lock (path packages have no `source`)."""
    return {
        m["name"]: m["version"]
        for m in LOCK_ENTRY.finditer(lock)
        if m["name"] in names and not m["next"].startswith("source = ")
    }


def bump_lock(lock: str, names: list[str], version: str) -> str:
    def replace(m: re.Match) -> str:
        if m["name"] not in names or m["next"].startswith("source = "):
            return m[0]
        return f'[[package]]\nname = "{m["name"]}"\nversion = "{version}"\n{m["next"]}'

    new = LOCK_ENTRY.sub(replace, lock)
    if set(lock_versions(new, names)) != set(names):
        raise ReleaseError(f"Cargo.lock: missing workspace crates {sorted(set(names) - set(lock_versions(new, names)))}")
    return new


def manifest_errors(root: Path = ROOT) -> tuple[str, list[str]]:
    """The workspace version, and every place that disagrees with it."""
    cargo = tomllib.loads((root / "Cargo.toml").read_text("utf-8"))
    version = cargo["workspace"]["package"]["version"]
    errors = []
    if not VERSION_RE.match(version):
        errors.append(f"Cargo.toml: workspace version {version!r} is not X.Y.Z or X.Y.Z-beta.N")
    for member in cargo["workspace"]["members"]:
        package = tomllib.loads((root / member / "Cargo.toml").read_text("utf-8"))["package"]
        if package.get("version") != {"workspace": True}:
            errors.append(f"{member}/Cargo.toml: use `version.workspace = true`, not {package.get('version')!r}")
    names = member_names(root)
    locked = lock_versions((root / "Cargo.lock").read_text("utf-8"), names)
    for name in names:
        if locked.get(name) != version:
            errors.append(f"Cargo.lock: {name} is {locked.get(name)!r}, workspace is {version!r}")
    errors += desktop_manifest_errors(root)
    return version, errors


def desktop_manifest_errors(root: Path = ROOT) -> list[str]:
    errors = []
    for rel in DESKTOP_MANIFESTS:
        path = root / rel
        if path.exists() and "version" in json.loads(path.read_text("utf-8")):
            errors.append(f"{rel}: remove `version`; the app takes the workspace version from its crate")
    return errors


# --- git, tools ------------------------------------------------------------------------


def run(*args: str, capture: bool = True) -> str:
    try:
        done = subprocess.run(args, cwd=ROOT, check=True, text=True, encoding="utf-8", capture_output=capture)
    except FileNotFoundError:
        raise ReleaseError(f"{args[0]} is not installed or not on PATH") from None
    except subprocess.CalledProcessError as e:
        raise ReleaseError(f"`{' '.join(args)}` failed:\n{(e.stderr or e.stdout or '').strip()}") from None
    return (done.stdout or "").strip()


def git_tags() -> list[str]:
    return run("git", "tag", "--list", "v*").splitlines()


def find_tool(name: str) -> str:
    """`name` from PATH, or from Python's script dirs (where `pip install --user` puts git-cliff)."""
    if found := shutil.which(name):
        return found
    for scheme in (sysconfig.get_default_scheme(), f"{os.name}_user"):
        try:
            scripts = Path(sysconfig.get_path("scripts", scheme))
        except KeyError:
            continue
        for exe in (scripts / name, scripts / f"{name}.exe"):
            if exe.is_file():
                return str(exe)
    raise ReleaseError(f"{name} not found; install it (`pip install {name}`)")


def trunk_errors(fetch: bool = True) -> list[str]:
    """Why HEAD isn't a clean, up-to-date `main` (empty when it is)."""
    errors = []
    branch = run("git", "rev-parse", "--abbrev-ref", "HEAD")
    if branch != MAIN:
        errors.append(f"on {branch!r}, not {MAIN!r}")
    if dirty := run("git", "status", "--porcelain"):
        errors.append("working tree is not clean:\n" + dirty)
    if fetch:
        # --force: the rolling `nightly` tag moves on every build (ADR-0019), and a plain
        # fetch refuses to update a tag that moved. Release tags never move.
        run("git", "fetch", "--quiet", "--force", "--tags", "origin", MAIN_REFSPEC)
        head, upstream = run("git", "rev-parse", "HEAD"), run("git", "rev-parse", f"origin/{MAIN}")
        if head != upstream:
            errors.append(f"HEAD {head[:7]} is not origin/{MAIN} {upstream[:7]}; pull first")
    return errors


# --- prepare ---------------------------------------------------------------------------


def render_changelog(version: str, tags: list[str]) -> str:
    args = [find_tool("git-cliff"), "--config", str(ROOT / "cliff.toml"), "--tag", f"v{version}"]
    if folded := folded_betas(tags, version):
        args += ["--ignore-tags", "^(" + "|".join(re.escape(t) for t in folded) + ")$"]
    # One blank line before each version heading (cliff.toml's `trim` removes it).
    text = re.sub(r"\n+(?=## \[)", "\n\n", run(*args, "--output", "-")) + "\n"
    if f"## [{version}]" not in text:
        raise ReleaseError(f"git-cliff produced no section for {version}")
    return text


def plan(version: str, tags: list[str]) -> dict[str, tuple[str, str]]:
    """{file: (current text, new text)} for every file the release PR changes."""
    read = lambda name: (ROOT / name).read_text("utf-8") if (ROOT / name).exists() else ""
    cargo, lock = read("Cargo.toml"), read("Cargo.lock")
    new = {
        "Cargo.toml": bump_cargo_toml(cargo, version),
        "Cargo.lock": bump_lock(lock, member_names(), version),
        "CHANGELOG.md": render_changelog(version, tags),
    }
    assert tuple(new) == RELEASE_FILES
    return {name: (read(name), text) for name, text in new.items()}


def print_plan(changes: dict[str, tuple[str, str]]) -> None:
    for name, (old, new) in changes.items():
        if old == new:
            print(f"--- {name}: unchanged")
            continue
        diff = difflib.unified_diff(old.splitlines(), new.splitlines(), f"a/{name}", f"b/{name}", lineterm="")
        print("\n".join(diff))


def prepare(version: str, dry_run: bool) -> int:
    refusals = trunk_errors(fetch=True)
    tags = git_tags()  # after the fetch, so remote tags count
    refusals += version_errors(version, tags)
    current, manifest = manifest_errors()
    refusals += manifest
    branch = f"release/v{version}"
    if run("git", "branch", "--list", branch) or run("git", "ls-remote", "--heads", "origin", branch):
        refusals.append(f"branch {branch} already exists")
    changes = plan(version, tags)

    if dry_run:
        print(f"release v{version} (from {current}){' [prerelease]' if is_prerelease(version) else ''}")
        for r in refusals:
            print(f"would refuse: {r}")
        print(f"would create branch {branch}, change {', '.join(n for n, (o, t) in changes.items() if o != t)},")
        print(f"commit and open PR `chore(release): v{version}` against {MAIN}\n")
        print_plan(changes)
        return 0
    if refusals:
        raise ReleaseError("refusing to release:\n- " + "\n- ".join(refusals))

    find_tool("gh")
    run("git", "switch", "--quiet", "-c", branch)
    for name, (_, text) in changes.items():
        (ROOT / name).write_bytes(text.encode("utf-8"))
    bumped, errors = manifest_errors()
    if errors or bumped != version:
        raise ReleaseError("version bump left manifests inconsistent:\n" + "\n".join(errors))
    title = f"chore(release): v{version}"
    run("git", "add", *RELEASE_FILES)
    run("git", "commit", "--quiet", "-m", title)
    run("git", "push", "--quiet", "-u", "origin", branch)
    kind = "beta prerelease" if is_beta(version) else "release"
    body = (
        f"Release PR for the v{version} {kind} (ADR-0013), opened by `scripts/release.py`.\n\n"
        f"- Bumps the workspace version {current} → {version}\n"
        "- Regenerates `CHANGELOG.md` with git-cliff\n\n"
        f"After this is squash-merged, run `python -I scripts/release.py tag {version}` on the "
        "updated `main`. The tag builds the Windows zip and publishes the GitHub Release."
    )
    url = run("gh", "pr", "create", "--base", MAIN, "--head", branch, "--title", title, "--body", body)
    print(f"opened {url}")
    return 0


# --- tag -------------------------------------------------------------------------------


def tag(version: str) -> int:
    refusals = trunk_errors(fetch=True) + version_errors(version, git_tags())
    current, errors = manifest_errors()
    refusals += errors
    if current != version:
        refusals.append(f"workspace version is {current}, not {version}; merge the release PR first")
    subject = run("git", "log", "-1", "--format=%s")
    if not subject.startswith(f"chore(release): v{version}"):
        refusals.append(f"tip of {MAIN} is {subject!r}, not the chore(release): v{version} commit")
    if refusals:
        raise ReleaseError("refusing to tag:\n- " + "\n- ".join(refusals))
    run("git", "tag", "-a", f"v{version}", "-m", f"Bolo v{version}")
    run("git", "push", "--quiet", "origin", f"v{version}")
    print(f"pushed v{version}; the release workflow now builds and publishes it")
    return 0


# --- check, notes ----------------------------------------------------------------------


def check(tag_name: str | None, on_main: bool) -> int:
    version, errors = manifest_errors()
    if tag_name:
        if parse_tag(tag_name) != version:
            errors.append(f"tag {tag_name} does not match workspace version {version}")
        if on_main:
            run("git", "fetch", "--quiet", "origin", MAIN_REFSPEC)
            ancestor = subprocess.run(["git", "merge-base", "--is-ancestor", "HEAD", f"origin/{MAIN}"], cwd=ROOT)
            if ancestor.returncode != 0:
                errors.append(f"{tag_name} is not on {MAIN}")
    if errors:
        raise ReleaseError("version check failed:\n- " + "\n- ".join(errors))
    print(f"ok: version {version} is consistent" + (f" with {tag_name}" if tag_name else ""))
    return 0


def changelog_section(changelog: str, version: str) -> str:
    """The body of `## [version]` in CHANGELOG.md, up to the next version heading."""
    m = re.search(rf"^## \[{re.escape(version)}\][^\n]*\n(.*?)(?=^## \[|\Z)", changelog, re.M | re.S)
    if not m or not m[1].strip():
        raise ReleaseError(f"CHANGELOG.md has no entries for {version}")
    return m[1].strip() + "\n"


def notes(tag_name: str, output: str | None) -> int:
    text = changelog_section((ROOT / "CHANGELOG.md").read_text("utf-8"), parse_tag(tag_name))
    if output:
        Path(output).write_bytes(text.encode("utf-8"))
    else:
        sys.stdout.buffer.write(text.encode("utf-8"))
    return 0


# --- package, publish ------------------------------------------------------------------

PACKAGING = ROOT / "packaging" / "windows"


def installer_name(version: str) -> str:
    """The desktop installer as published. A nightly is the same app on its Nightly
    channel (ADR-0019); its installer is published as `Bolo-Nightly_...` so the download
    says which channel it is."""
    product = "Bolo-Nightly" if NIGHTLY_RE.match(version) else "Bolo"
    return f"{product}_{version}_x64-setup.exe"


def write_sha256(path: Path) -> str:
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    # Bytes, not write_text: on Windows that writes CRLF, and `sha256sum -c` then reads the
    # CR as part of the file name.
    path.with_name(path.name + ".sha256").write_bytes(f"{digest}  {path.name}\n".encode())
    return digest


def package(tag_name: str) -> int:
    """dist/bolo-vX-windows-x64.zip (one top-level folder), dist/Bolo_X_x64-setup.exe,
    and a .sha256 for each."""
    version = parse_tag(tag_name)
    installer = ROOT / "target" / "release" / "bundle" / "nsis" / installer_name(version)
    if not installer.is_file():
        raise ReleaseError(f"{installer} is missing; run `npm ci && npx tauri build` in apps/desktop")
    exe = ROOT / "target" / "release" / "bolo.exe"
    dlls = sorted((ROOT / ".cache" / "sherpa-lib").glob("*.dll"))
    if not exe.is_file():
        raise ReleaseError(f"{exe} is missing; run `cargo build --release --locked -p bolo`")
    if not dlls:
        raise ReleaseError("no sherpa-onnx DLLs in .cache/sherpa-lib; run scripts/fetch-sherpa-libs.py")
    stem = f"bolo-{tag_name}-windows-x64"
    readme = (PACKAGING / "README.txt").read_text("utf-8").replace("{version}", version)
    files = {
        "bolo.exe": exe.read_bytes(),
        **{d.name: d.read_bytes() for d in dlls},
        "README.txt": readme.encode("utf-8"),
        "LICENSE-MIT.txt": (ROOT / "LICENSE-MIT").read_bytes(),
        "LICENSE-APACHE.txt": (ROOT / "LICENSE-APACHE").read_bytes(),
        "THIRD-PARTY-NOTICES.txt": (PACKAGING / "THIRD-PARTY-NOTICES.txt").read_bytes(),
    }
    dist = ROOT / "dist"
    dist.mkdir(exist_ok=True)
    archive = dist / f"{stem}.zip"
    with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
        for name, data in files.items():
            z.writestr(f"{stem}/{name}", data)
    copied = dist / installer.name
    shutil.copyfile(installer, copied)
    signature = installer.with_name(installer.name + ".sig")
    if signature.is_file():  # updater artifact, written when the signing key is set
        shutil.copyfile(signature, dist / signature.name)
    for path in (archive, copied):
        digest = write_sha256(path)
        print(f"{path.relative_to(ROOT)}: {path.stat().st_size} bytes, sha256 {digest}")
    return 0


def sha256(paths: list[str]) -> int:
    for name in paths:
        path = Path(name)
        if not path.is_file():
            raise ReleaseError(f"{path} is missing")
        print(f"{path.name}: sha256 {write_sha256(path)}")
    return 0


def feed_json(version: str, notes: str, signature: str, url: str, pub_date: datetime.datetime) -> dict:
    """Tauri's static updater feed for one Windows x64 installer."""
    return {
        "version": version,
        "notes": notes,
        "pub_date": pub_date.astimezone(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "platforms": {"windows-x86_64": {"signature": signature.strip(), "url": url}},
    }


def feed(tag_name: str, url_base: str, notes_text: str | None = None) -> int:
    """dist/latest.json for the installer (and its .sig) that `package` put in dist/."""
    version = parse_tag(tag_name)
    dist = ROOT / "dist"
    name = installer_name(version)
    sig = dist / (name + ".sig")
    if not sig.is_file():
        raise ReleaseError(f"{sig} is missing; build with TAURI_SIGNING_PRIVATE_KEY set, then `package`")
    if notes_text is None:
        notes_text = changelog_section((ROOT / "CHANGELOG.md").read_text("utf-8"), version)
    doc = feed_json(version, notes_text, sig.read_text("utf-8"), f"{url_base.rstrip('/')}/{name}",
                    datetime.datetime.now(datetime.timezone.utc))
    out = dist / "latest.json"
    out.write_bytes((json.dumps(doc, indent=2) + "\n").encode("utf-8"))
    print(f"{out.relative_to(ROOT)}: {version} -> {doc['platforms']['windows-x86_64']['url']}")
    return 0


def set_version(version: str) -> int:
    """Write a nightly version into Cargo.toml and Cargo.lock for one CI build. The change
    is never committed: the nightly workflow builds from a throwaway checkout."""
    if not NIGHTLY_RE.match(version):
        raise ReleaseError(f"not a nightly version: {version!r} (releases go through `prepare`)")
    for name, bump in (("Cargo.toml", lambda t: bump_cargo_toml(t, version)),
                       ("Cargo.lock", lambda t: bump_lock(t, member_names(ROOT), version))):
        path = ROOT / name
        path.write_bytes(bump(path.read_text("utf-8")).encode("utf-8"))
    print(f"version set to {version} (not committed)")
    return 0


def publish(tag_name: str, assets: list[str]) -> int:
    version = parse_tag(tag_name)
    notes_file = ROOT / "dist" / "release-notes.md"
    notes_file.parent.mkdir(exist_ok=True)
    notes(tag_name, str(notes_file))
    args = ["gh", "release", "create", tag_name, *assets, "--verify-tag"]
    args += ["--title", f"Bolo {tag_name}", "--notes-file", str(notes_file)]
    if is_prerelease(version):
        args.append("--prerelease")
    else:
        args.append("--latest")
    print(run(*args))
    return 0


# --- cli -------------------------------------------------------------------------------


def main(argv: list[str]) -> int:
    if argv and VERSION_RE.match(argv[0]):
        argv = ["prepare", *argv]
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = parser.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("prepare", help="open the chore(release) PR")
    p.add_argument("version")
    p.add_argument("--dry-run", action="store_true", help="print the planned changes; change nothing")
    sub.add_parser("tag", help="tag the merged release commit").add_argument("version")
    p = sub.add_parser("check", help="version strings agree across manifests")
    p.add_argument("--tag", dest="tag_name", help="also require this tag to match")
    p.add_argument("--on-main", action="store_true", help="also require HEAD to be on origin/main")
    p = sub.add_parser("notes", help="print a version's changelog section")
    p.add_argument("tag_name")
    p.add_argument("--output")
    sub.add_parser("package", help="zip the Windows CLI and collect the installer").add_argument("tag_name")
    p = sub.add_parser("feed", help="write dist/latest.json, the updater feed")
    p.add_argument("tag_name")
    p.add_argument("--url-base", required=True, help="where the installer is downloaded from")
    p.add_argument("--notes", help="notes text instead of the CHANGELOG.md section (nightlies)")
    p = sub.add_parser("sha256", help="write FILE.sha256 for each file (after signing)")
    p.add_argument("paths", nargs="+")
    p = sub.add_parser("nightly-version", help="print the version for a nightly build")
    p.add_argument("--run", type=int, required=True, help="the CI run number")
    sub.add_parser("set-version", help="write a nightly version into the manifests").add_argument("version")
    p = sub.add_parser("publish", help="create the GitHub Release")
    p.add_argument("tag_name")
    p.add_argument("assets", nargs="+")
    a = parser.parse_args(argv)
    try:
        match a.cmd:
            case "prepare":
                return prepare(a.version, a.dry_run)
            case "tag":
                return tag(a.version)
            case "check":
                return check(a.tag_name, a.on_main)
            case "notes":
                return notes(a.tag_name, a.output)
            case "package":
                return package(a.tag_name)
            case "feed":
                return feed(a.tag_name, a.url_base, a.notes)
            case "sha256":
                return sha256(a.paths)
            case "nightly-version":
                print(nightly_version(git_tags(), datetime.datetime.now(datetime.timezone.utc).date(), a.run))
                return 0
            case "set-version":
                return set_version(a.version)
            case "publish":
                return publish(a.tag_name, a.assets)
    except ReleaseError as e:
        print(f"error: {e}", file=sys.stderr)
        return 1
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
