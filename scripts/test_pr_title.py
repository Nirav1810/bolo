"""Tests for the PR-title lint (ADR-0013). Run: python -I -m unittest discover -s scripts"""

import os
import subprocess
import sys
import unittest
from pathlib import Path

from pr_title import is_conventional

SCRIPT = Path(__file__).with_name("pr_title.py")


def run_cli(title: str | None) -> subprocess.CompletedProcess:
    env = {k: v for k, v in os.environ.items() if k != "PR_TITLE"}
    if title is not None:
        env["PR_TITLE"] = title
    return subprocess.run(
        [sys.executable, "-I", str(SCRIPT)], env=env, capture_output=True, text=True
    )


class Cli(unittest.TestCase):
    def test_passes_a_conventional_title(self):
        self.assertEqual(run_cli("docs: add contributor note").returncode, 0)

    def test_fails_a_bad_title_and_explains_the_format(self):
        result = run_cli("Update README")
        self.assertEqual(result.returncode, 1)
        self.assertIn("Update README", result.stdout)
        self.assertIn("feat", result.stdout)

    def test_fails_when_no_title_is_given(self):
        self.assertNotEqual(run_cli(None).returncode, 0)


class ConventionalTitle(unittest.TestCase):
    def test_accepts_type_and_description(self):
        self.assertTrue(is_conventional("feat: add Auto language mode"))

    def test_accepts_every_allowed_type(self):
        for t in ["feat", "fix", "perf", "refactor", "docs", "test", "ci", "build", "chore", "revert"]:
            with self.subTest(type=t):
                self.assertTrue(is_conventional(f"{t}: something"))

    def test_accepts_optional_scope_and_breaking_marker(self):
        for title in [
            "fix(core): stop dictation on cancel",
            "chore(release): v0.1.0",
            "feat(bolo-stt)!: drop batch Deepgram",
            "refactor!: rename session to dictation",
        ]:
            with self.subTest(title=title):
                self.assertTrue(is_conventional(title))

    def test_rejects_malformed_titles(self):
        for title in [
            "Update README",
            "feat:no space",
            "feat:  ",
            "feat: ",
            "feat",
            "feat(): empty scope",
            "feat(core) : space before colon",
            " feat: leading space",
            "",
        ]:
            with self.subTest(title=title):
                self.assertFalse(is_conventional(title))

    def test_rejects_types_outside_the_list(self):
        for title in ["feature: add x", "style: reformat", "wip: stuff", "Feat: add x"]:
            with self.subTest(title=title):
                self.assertFalse(is_conventional(title))


if __name__ == "__main__":
    unittest.main()
