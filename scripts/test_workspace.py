"""Workspace-manifest invariants. Run: python -I -m unittest discover -s scripts"""

import tomllib
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def load(path: Path) -> dict:
    with path.open("rb") as f:
        return tomllib.load(f)


class Workspace(unittest.TestCase):
    def test_no_crate_can_be_published(self):
        """ADR-0013: crates are internal to the apps; `cargo publish` must refuse them all."""
        root = load(ROOT / "Cargo.toml")
        inherited = root["workspace"].get("package", {}).get("publish")
        members = root["workspace"]["members"]
        self.assertTrue(members)
        for member in members:
            with self.subTest(crate=member):
                publish = load(ROOT / member / "Cargo.toml")["package"].get("publish")
                if publish == {"workspace": True}:
                    publish = inherited
                self.assertIs(publish, False)


if __name__ == "__main__":
    unittest.main()
