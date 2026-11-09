# 01: Trunk guardrails

**What to build:** `main` changes only through squash-merged PRs that passed CI and whose title is a Conventional Commit. That keeps every commit on `main` releasable and gives the changelog clean input (ADR-0013). Workspace crates can't be published to crates.io by accident.

**Blocked by:** None (can start immediately)

**Status:** ready-for-agent

- [x] A CI workflow lints PR titles and fails on titles that don't conform. Allowed types: feat, fix, perf, refactor, docs, test, ci, build, chore, revert. The scope is optional.
- [x] The repo allows squash merges only, and the squash commit message defaults to the PR title.
- [x] A ruleset on `main` enforces:
  - a PR is required (0 approvals);
  - required checks: both rust jobs, deny, and the PR-title lint;
  - linear history;
  - no force pushes and no deletion.
- [x] Every workspace crate has `publish = false`.
- [x] This ticket lands through its own PR. The last direct push to `main` is the commit that adds this ticket set. A test PR with a bad title is shown failing the lint.
- [x] README has a contributor note: branch → PR → squash merge, with Conventional Commit titles.

## Comments

**2026-11-09: implemented in #1**
- `pr-title` workflow plus `scripts/pr_title.py`, with tests in `scripts/test_pr_title.py`. The rust CI job runs the script tests.
- Crates: `publish = false` in `[workspace.package]`, inherited by all 8 members. `scripts/test_workspace.py` guards it. `cargo metadata` reports `publish: []` for every crate.
- Repo settings: squash merge only. The squash commit title is the PR title and the body is blank, so the commit on `main` is just the title. Branches are deleted on merge.
- Ruleset `main` (id 26847972) targets the default branch and has no bypass actors:
  - deletion, non_fast_forward, required_linear_history;
  - a pull request with 0 approvals, squash only;
  - required checks with strict up-to-date: `rust (windows-latest)`, `rust (ubuntu-latest)`, `deny`, `pr-title`.
- The last direct push to `main` is f8194d2 (docs: glossary, ADR-0013/0014, M1-finish spec and tickets).
- Bad-title demo: #2 "Update README" failed `pr-title` (https://github.com/Nirav1810/bolo/actions/runs/42610314110) and was closed unmerged.
