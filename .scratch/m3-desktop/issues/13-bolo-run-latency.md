# 13: Investigate bolo run's Nemotron finish-only latency

**What to build:** M2 gap 3. `bolo run` reports Nemotron finish-only latency of 285–295 ms p50, against 158 ms in M1. Find the cause and fix it, or explain it.

**Blocked by:** none

**Status:** ready-for-agent

- [ ] Bisect between M1's `v0.1.0` and `e087099` with A/B binaries (`target/release/bolo-<name>.exe`, one run at a time, per the dev-machine notes).
- [ ] If it's a code change, fix it or document why it's correct (for example, the tail pad from M2 ticket 01 counting in `bolo run`'s clock).
- [ ] If it's the machine, show it with the M1 binary on today's machine.
- [ ] BENCH.md gets a short section with the finding.
