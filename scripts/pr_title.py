#!/usr/bin/env python3
"""Lint a PR title as a Conventional Commit (ADR-0013).

Squash merges use the PR title as the commit on `main`, and git-cliff builds the changelog
from those commits, so the title is what we lint. The pr-title workflow runs this.

Usage: PR_TITLE="feat(core): add X" python -I scripts/pr_title.py
"""

import os
import re
import sys

TYPES = ("feat", "fix", "perf", "refactor", "docs", "test", "ci", "build", "chore", "revert")

# type, optional (scope), optional ! for breaking changes, ": ", non-blank description.
CONVENTIONAL_TITLE = re.compile(rf"^(?:{'|'.join(TYPES)})(?:\([\w./-]+\))?!?: \S")


def is_conventional(title: str) -> bool:
    return CONVENTIONAL_TITLE.match(title) is not None


def main() -> int:
    title = os.environ.get("PR_TITLE")
    if title is None:
        print("::error::PR_TITLE is not set")
        return 2
    if is_conventional(title):
        print(f"ok: {title}")
        return 0
    # `::error::` makes GitHub Actions show this as an annotation on the PR's checks.
    print(f"::error::PR title is not a Conventional Commit: {title!r}")
    print(f"Expected `type(scope)!: description`; types: {', '.join(TYPES)}.")
    print("Scope and ! are optional, e.g. `feat(core): add Auto language mode`.")
    return 1


if __name__ == "__main__":
    sys.exit(main())
