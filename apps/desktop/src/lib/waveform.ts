// The overlay pill's waveform.
//
// A row of bars that breathe with the voice: the mic level (the engine's `on_level` RMS,
// −50…−10 dBFS) drives a smoothed envelope, and every bar follows it, tallest in the
// middle, each wobbling on its own slow phase so speech looks alive rather than like a
// meter. Each bar eases toward its target, so nothing jumps from one frame to the next,
// however unevenly levels arrive. In silence the bars settle into a small idle ripple.

export const BARS = 13;
export const MIN = 3;
export const MAX = 22;
const FLOOR_DB = -50;
const CEIL_DB = -10;
/** Envelope time constants: it rises quickly and falls gently. */
const RISE_MS = 35;
const FALL_MS = 180;
/** How quickly each bar chases its own target. */
const BAR_MS = 55;

/** The level (linear RMS) as 0–1 of the waveform's range. */
export function loudness(rms: number): number {
  const db = 20 * Math.log10(Math.max(rms, 1e-9));
  if (db <= FLOOR_DB) return 0;
  return Math.min(1, (db - FLOOR_DB) / (CEIL_DB - FLOOR_DB));
}

/** Tallest in the middle, about half as tall at the ends. */
const shape = Array.from({ length: BARS }, (_, i) => {
  const x = (i - (BARS - 1) / 2) / ((BARS - 1) / 2);
  return 0.5 + 0.5 * Math.exp(-2.2 * x * x);
});

export interface Wave {
  env: number;
  bars: number[];
}

export const resting = (): Wave => ({ env: 0, bars: Array(BARS).fill(MIN) });

/** Exponential approach of `from` to `to` over `dt` with time constant `tau`. */
const approach = (from: number, to: number, dt: number, tau: number) => to + (from - to) * Math.exp(-dt / tau);

/**
 * Advances the wave by one display frame: `dt` ms after the last, at time `t` ms, with
 * the latest `level` (0–1). Mutates and returns `w`.
 */
export function step(w: Wave, level: number, t: number, dt: number): Wave {
  w.env = approach(w.env, level, dt, level > w.env ? RISE_MS : FALL_MS);
  for (let i = 0; i < BARS; i++) {
    const wobble = 0.72 + 0.28 * Math.sin(t / (150 + i * 23) + i * 1.9);
    const idle = 1.2 * (0.5 + 0.5 * Math.sin(t / 260 - i * 0.55));
    const target = MIN + idle + (MAX - MIN - idle) * w.env * shape[i] * wobble;
    w.bars[i] = approach(w.bars[i], target, dt, BAR_MS);
  }
  return w;
}
