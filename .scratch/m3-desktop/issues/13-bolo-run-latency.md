# 13: Investigate bolo run's Nemotron finish-only latency

**What to build:** M2 gap 3. `bolo run` reports Nemotron finish-only latency of 285–295 ms p50, against 158 ms in M1. Find the cause and fix it, or explain it.

**Blocked by:** none

**Status:** done

- [x] Bisect between M1's `v0.1.0` and `e087099` with A/B binaries (`target/release/bolo-<name>.exe`, one run at a time, per the dev-machine notes).
- [x] If it's a code change, fix it or document why it's correct (for example, the tail pad from M2 ticket 01 counting in `bolo run`'s clock).
- [x] If it's the machine, show it with the M1 binary on today's machine.
- [x] BENCH.md gets a short section with the finding.

## Comments

**2026-11-11 (agent).** BENCH.md §20. I ran `v0.1.0`, the ticket 01 build (`465954d`) and the M2 final (`e087099`) back to back today. They read 226, 228 and 235 ms p50. So no code change caused the gap: the M1 binary itself no longer reproduces its 158 ms on this machine. The cause is the machine, not a code change; nothing to fix.
