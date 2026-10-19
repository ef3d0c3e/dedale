// Numbers and words as the terminal shows them (crates/dedale/src/render.rs
// and cli/summary.rs), so both views read the same.

import type { GameResult, Outcome, Seating } from "./types.ts";

export const LETTERS = ["A", "B"] as const;

/** Player `i`'s letter. */
export function letter(player: number): string {
  return LETTERS[player] ?? String(player + 1);
}

/** Microseconds: `850 µs`, `12.3 ms`, `1.05 s`; `-` when unknown. */
export function formatUs(us: number | null | undefined): string {
  if (us === null || us === undefined) return "-";
  if (us < 1000) return `${us} µs`;
  if (us < 1_000_000) return `${Math.floor(us / 1000)}.${Math.floor((us % 1000) / 100)} ms`;
  const hundredths = Math.floor((us % 1_000_000) / 10_000);
  return `${Math.floor(us / 1_000_000)}.${String(hundredths).padStart(2, "0")} s`;
}

/** A score in half-points, as points: `3`, `3.5`. */
export function score(halfPoints: number): string {
  return halfPoints % 2 === 0 ? String(halfPoints / 2) : `${Math.floor(halfPoints / 2)}.5`;
}

export function outcomeName(outcome: Outcome): string {
  return outcome === "not_ready" ? "not ready" : outcome;
}

/** How a game ended, with its winner: `cleared on turn 102: A greedy wins, 60 to 12`. */
export function describeResult(
  result: GameResult,
  seating: Seating,
  names: readonly string[],
): string {
  const turns = result.turns;
  const ending =
    result.reason === "cleared"
      ? `cleared on turn ${turns}`
      : result.reason === "turn_limit"
        ? `turn limit reached after ${turns} turns`
        : `abandoned on turn ${turns}: every bot crashed`;
  const scores = result.half_points.map(score);
  if (seating === "solo") return `${ending}, ${scores[0] ?? "0"} points`;
  if (result.winner === null) return `${ending}: draw, ${scores[0]} to ${scores[1]}`;
  const winner = result.winner;
  return `${ending}: ${letter(winner)} ${names[winner] ?? ""} wins, ${scores[winner]} to ${scores[1 - winner]}`;
}
