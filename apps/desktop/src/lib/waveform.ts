// The overlay pill's waveform (design: M3 ticket 04).
//
// 16 bars, newest on the right, one bar pushed in per display frame. The level is the
// engine's `on_level` RMS, mapped from −50…−10 dBFS onto 3–24 px. Below −50 dBFS is
// silence: bars rest at 3 px and, once they all do, nothing moves at all. The input
// envelope rises at once and falls over 120 ms.

export const BARS = 16;
export const MIN = 3;
export const MAX = 24;
const FLOOR_DB = -50;
const CEIL_DB = -10;
const FALL_MS = 120;

/** A bar's height in px for an RMS level (linear, 0–1). */
export function barHeight(rms: number): number {
  const db = 20 * Math.log10(Math.max(rms, 1e-9));
  if (db < FLOOR_DB) return MIN;
  const t = Math.min(1, (db - FLOOR_DB) / (CEIL_DB - FLOOR_DB));
  return MIN + t * (MAX - MIN);
}

export const resting = (): number[] => Array(BARS).fill(MIN);

/**
 * One display frame, `dtMs` after the last: the envelope `head` follows `target` (up at
 * once, down over 120 ms) and is pushed in on the right. `null` when nothing would move.
 */
export function step(bars: number[], head: number, target: number, dtMs: number): { bars: number[]; head: number } | null {
  const next = target >= head ? target : Math.max(target, head - ((MAX - MIN) * dtMs) / FALL_MS);
  if (next <= MIN && bars.every((b) => b <= MIN)) return null;
  return { bars: [...bars.slice(1), next], head: next };
}
