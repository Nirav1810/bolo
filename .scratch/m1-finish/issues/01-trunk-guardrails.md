# 01: Trunk guardrails

**What to build:** `main` changes only through squash-merged PRs that passed CI and whose title is a Conventional Commit. That keeps every commit on `main` releasable and gives the changelog clean input (ADR-0013). Workspace crates can't be published to crates.io by accident.

**Blocked by:** None (can start immediately)

**Status:** ready-for-agent

- [ ] A CI workflow lints PR titles and fails on titles that don't conform. Allowed types: feat, fix, perf, refactor, docs, test, ci, build, chore, revert. The scope is optional.
- [ ] The repo allows squash merges only, and the squash commit message defaults to the PR title.
- [ ] A ruleset on `main` enforces:
  - a PR is required (0 approvals);
  - required checks: both rust jobs, deny, and the PR-title lint;
  - linear history;
  - no force pushes and no deletion.
- [ ] Every workspace crate has `publish = false`.
- [ ] This ticket lands through its own PR. The last direct push to `main` is the commit that adds this ticket set. A test PR with a bad title is shown failing the lint.
- [ ] README has a contributor note: branch → PR → squash merge, with Conventional Commit titles.
