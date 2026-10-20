// Values per turn for the charts, their spread, and how to fit them on screen.

import type { TurnRecord } from "../types.ts";

export type Metric = "latency_us" | "think_us" | "half_points";

/** One value per turn (turn `i + 1` at index `i`); `null` when there was none. */
export function series(
  turns: readonly TurnRecord[],
  player: number,
  metric: Metric,
): (number | null)[] {
  return turns.map((turn) => turn.players[player]?.[metric] ?? null);
}

/** Durations for a logarithmic axis, which has no 0: less than 1 µs reads as 1 µs. */
export function forLog(values: readonly (number | null)[]): (number | null)[] {
  return values.map((v) => (v === null ? null : Math.max(1, v)));
}

/** Points (not half-points) per turn. */
export function points(values: readonly (number | null)[]): (number | null)[] {
  return values.map((v) => (v === null ? null : v / 2));
}

/**
 * Where to draw `player`'s problems: at each turn whose outcome was not
 * `ok`, `at(index)`; `null` on the other turns.
 */
export function problems(
  turns: readonly TurnRecord[],
  player: number,
  at: (index: number) => number,
): (number | null)[] {
  return turns.map((turn, i) => {
    const outcome = turn.players[player]?.outcome;
    return outcome === undefined || outcome === "ok" ? null : at(i);
  });
}

/** Points still in the maze after each turn, from `total` at the start. */
export function pointsLeft(total: number, turns: readonly TurnRecord[]): number[] {
  let left = total;
  return turns.map((turn) => {
    left -= turn.collected.length;
    return left;
  });
}

/** Average and slowest replies of some durations. */
export interface Spread {
  /** Rounded down. */
  mean: number;
  /** The slowest 5% take at least this long. */
  p95: number;
  /** The slowest 1% take at least this long. */
  p99: number;
}

/**
 * The spread of the values that are there, computed as in the game's
 * statistics (nearest-rank percentiles); `undefined` without any.
 */
export function spread(values: readonly (number | null)[]): Spread | undefined {
  const sorted = values.filter((v): v is number => v !== null).sort((a, b) => a - b);
  const n = sorted.length;
  if (n === 0) return undefined;
  const sum = sorted.reduce((total, v) => total + v, 0);
  // Rank ceil(p·n/100), 1-based.
  const rank = (p: number): number => sorted[Math.max(1, Math.ceil((p * n) / 100)) - 1] ?? 0;
  return { mean: Math.floor(sum / n), p95: rank(95), p99: rank(99) };
}

/** Powers of ten from `min` to `max`: the lines of a logarithmic axis. */
export function powersOfTen(min: number, max: number): number[] {
  const out: number[] = [];
  for (let v = 10 ** Math.ceil(Math.log10(Math.max(1, min))); v <= max; v *= 10) out.push(v);
  return out;
}

/** A power of ten of microseconds as an axis label: `10 µs`, `1 ms`, `1 s`. */
export function durationTick(us: number): string {
  if (us < 1000) return `${us} µs`;
  if (us < 1_000_000) return `${us / 1000} ms`;
  return `${us / 1_000_000} s`;
}
