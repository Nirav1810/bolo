"""Tests for scripts/release.py. Run: python -I -m unittest discover -s scripts

The dry-run test needs git-cliff (`pip install git-cliff`); CI installs it, and there a
missing git-cliff fails the test instead of skipping it.
"""

import io
import os
import tomllib
import unittest
import zipfile
from contextlib import redirect_stdout
from pathlib import Path
from unittest import mock

import release
from release import ReleaseError

ROOT = Path(__file__).resolve().parent.parent

LOCK = """\
version = 4

[[package]]
name = "anyhow"
version = "1.0.0"
source = "registry+https://github.com/rust-lang/crates.io-index"

[[package]]
name = "bolo"
version = "0.1.0"
dependencies = [
 "bolo-core",
]

[[package]]
name = "bolo-core"
version = "0.1.0"

[[package]]
name = "bolo-core"
version = "0.1.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
"""


class Versions(unittest.TestCase):
    def test_accepts_stable_and_beta_versions_only(self):
        for ok in ("0.1.0", "1.20.3", "0.1.0-beta.1", "2.0.0-beta.12"):
            release.version_key(ok)
        for bad in ("v0.1.0", "0.1", "01.0.0", "0.1.0-rc.1", "0.1.0-beta.0", "0.1.0-beta", "0.1.0+build"):
            with self.subTest(bad=bad), self.assertRaises(ReleaseError):
                release.version_key(bad)

    def test_betas_sort_before_their_stable_release(self):
        ordered = ["0.1.0-beta.1", "0.1.0-beta.2", "0.1.0-beta.10", "0.1.0", "0.1.1-beta.1", "0.1.1", "0.2.0"]
        self.assertEqual(sorted(reversed(ordered), key=release.version_key), ordered)

    def test_prerelease_means_beta_or_pre_1_0(self):
        self.assertTrue(release.is_prerelease("1.0.0-beta.1"))
        self.assertTrue(release.is_prerelease("0.1.0"))  # unsigned CLI preview (ADR-0013)
        self.assertFalse(release.is_prerelease("1.0.0"))

    def test_new_version_must_be_untagged_and_newer(self):
        tags = ["v0.1.0-beta.1", "not-a-release", "v0.0.9"]
        self.assertEqual(release.version_errors("0.1.0-beta.2", tags), [])
        self.assertEqual(release.version_errors("0.1.0", tags), [])
        self.assertIn("already tagged", release.version_errors("0.1.0-beta.1", tags)[0])
        self.assertIn("not newer", release.version_errors("0.0.10", tags)[0])
        self.assertEqual(release.version_errors("0.0.1", []), [])

    def test_stable_release_folds_its_betas_and_older_ones(self):
        tags = ["v0.1.0-beta.1", "v0.1.0-beta.2", "v0.1.0", "v0.2.0-beta.1"]
        self.assertEqual(release.folded_betas(tags, "0.2.0-beta.2"), ["v0.1.0-beta.1", "v0.1.0-beta.2"])
        self.assertEqual(release.folded_betas(tags, "0.2.0"), ["v0.1.0-beta.1", "v0.1.0-beta.2", "v0.2.0-beta.1"])
        self.assertEqual(release.folded_betas(["v0.1.0-beta.1"], "0.1.0-beta.2"), [])


class Manifests(unittest.TestCase):
    def test_bumps_only_the_workspace_package_version(self):
        text = (ROOT / "Cargo.toml").read_text("utf-8")
        bumped = release.bump_cargo_toml(text, "9.8.7-beta.6")
        self.assertEqual(release.workspace_version(bumped), "9.8.7-beta.6")
        changed = [(a, b) for a, b in zip(text.splitlines(), bumped.splitlines()) if a != b]
        self.assertEqual(len(changed), 1)
        self.assertEqual(tomllib.loads(bumped)["workspace"]["dependencies"], tomllib.loads(text)["workspace"]["dependencies"])

    def test_bumps_workspace_crates_in_the_lockfile_not_registry_ones(self):
        bumped = release.bump_lock(LOCK, ["bolo", "bolo-core"], "0.2.0")
        self.assertEqual(release.lock_versions(bumped, ["bolo", "bolo-core"]), {"bolo": "0.2.0", "bolo-core": "0.2.0"})
        self.assertIn('name = "anyhow"\nversion = "1.0.0"', bumped)
        self.assertIn('name = "bolo-core"\nversion = "0.1.0"\nsource', bumped)

    def test_lockfile_must_hold_every_workspace_crate(self):
        with self.assertRaises(ReleaseError):
            release.bump_lock(LOCK, ["bolo", "bolo-ghost"], "0.2.0")

    def test_repo_manifests_agree_on_one_version(self):
        version, errors = release.manifest_errors()
        self.assertEqual(errors, [])
        release.version_key(version)


class Changelog(unittest.TestCase):
    CHANGELOG = """\
# Changelog

## [0.2.0-beta.1] - 2026-12-01

### Features

- **core:** New thing

## [0.1.0] - 2026-11-20

### Bug fixes

- Old fix
"""

    def test_cuts_one_version_section(self):
        self.assertEqual(release.changelog_section(self.CHANGELOG, "0.2.0-beta.1"), "### Features\n\n- **core:** New thing\n")
        self.assertEqual(release.changelog_section(self.CHANGELOG, "0.1.0"), "### Bug fixes\n\n- Old fix\n")

    def test_missing_version_is_an_error_not_empty_notes(self):
        with self.assertRaises(ReleaseError):
            release.changelog_section(self.CHANGELOG, "0.1.0-beta.1")

    def test_cliff_config_skips_release_commits(self):
        parsers = tomllib.loads((ROOT / "cliff.toml").read_text("utf-8"))["git"]["commit_parsers"]
        self.assertEqual(parsers[0], {"message": r"^chore\(release\)", "skip": True})


class DryRun(unittest.TestCase):
    """The release PR changes exactly the manifests and the changelog, consistently."""

    def setUp(self):
        try:
            release.find_tool("git-cliff")
        except ReleaseError:
            if os.environ.get("CI"):
                raise
            self.skipTest("git-cliff not installed")

    def test_plan_bumps_every_manifest_and_writes_the_changelog(self):
        changes = release.plan("99.0.0-beta.1", release.git_tags())
        self.assertEqual(tuple(changes), release.RELEASE_FILES)
        cargo, lock, changelog = (new for _, new in changes.values())
        self.assertEqual(release.workspace_version(cargo), "99.0.0-beta.1")
        versions = release.lock_versions(lock, release.member_names())
        self.assertEqual(set(versions.values()), {"99.0.0-beta.1"})
        self.assertIn("## [99.0.0-beta.1] - ", changelog)
        self.assertNotIn("chore(release)", changelog)
        self.assertTrue(release.changelog_section(changelog, "99.0.0-beta.1"))

    def test_dry_run_changes_nothing(self):
        before = {name: (ROOT / name).read_bytes() if (ROOT / name).exists() else None for name in release.RELEASE_FILES}
        out = io.StringIO()
        with redirect_stdout(out):
            self.assertEqual(release.main(["99.0.0-beta.1", "--dry-run"]), 0)
        self.assertIn("chore(release): v99.0.0-beta.1", out.getvalue())
        after = {name: (ROOT / name).read_bytes() if (ROOT / name).exists() else None for name in release.RELEASE_FILES}
        self.assertEqual(before, after)


class Package(unittest.TestCase):
    def test_zip_has_one_folder_with_exe_dlls_readme_and_licences(self):
        import tempfile

        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            (tmp / "target" / "release").mkdir(parents=True)
            (tmp / "target" / "release" / "bolo.exe").write_bytes(b"MZ")
            (tmp / ".cache" / "sherpa-lib").mkdir(parents=True)
            for dll in ("onnxruntime.dll", "sherpa-onnx-c-api.dll", "sherpa-onnx-c-api.lib"):
                (tmp / ".cache" / "sherpa-lib" / dll).write_bytes(b"x")
            for name in ("LICENSE-MIT", "LICENSE-APACHE"):
                (tmp / name).write_bytes((ROOT / name).read_bytes())
            with mock.patch.object(release, "ROOT", tmp), redirect_stdout(io.StringIO()):
                release.package("v0.1.0-beta.1")
            stem = "bolo-v0.1.0-beta.1-windows-x64"
            with zipfile.ZipFile(tmp / "dist" / f"{stem}.zip") as z:
                names = sorted(z.namelist())
                readme = z.read(f"{stem}/README.txt").decode()
            expected = ["LICENSE-APACHE.txt", "LICENSE-MIT.txt", "README.txt", "THIRD-PARTY-NOTICES.txt",
                        "bolo.exe", "onnxruntime.dll", "sherpa-onnx-c-api.dll"]
            self.assertEqual(names, sorted(f"{stem}/{n}" for n in expected))
            self.assertIn("Bolo 0.1.0-beta.1", readme)
            self.assertTrue((tmp / "dist" / f"{stem}.zip.sha256").read_text().endswith(f"  {stem}.zip\n"))


if __name__ == "__main__":
    unittest.main()
