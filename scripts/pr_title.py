"""Lint a PR title as a Conventional Commit (ADR-0013).

Squash merges use the PR title as the commit subject on `main`, and git-cliff builds the
changelog from those subjects, so the title is what we lint. CI passes it via $PR_TITLE.
"""

import os
import re
import sys

TYPES = ("feat", "fix", "perf", "refactor", "docs", "test", "ci", "build", "chore", "revert")

# type, optional (scope), optional ! for breaking changes, ": ", non-blank description.
PATTERN = re.compile(rf"^(?:{'|'.join(TYPES)})(?:\([\w./-]+\))?!?: \S")


def is_conventional(title: str) -> bool:
    return PATTERN.match(title) is not None


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
