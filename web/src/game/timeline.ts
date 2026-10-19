// Problems along a game: every turn a player did not answer `ok`, and every
// late answer. "Next problem" jumps to where one starts, so a crashed
// player's long run of `crashed` turns counts once.

import type { EventRecord, Outcome, TurnRecord } from "../types.ts";

export type MarkerKind = Exclude<Outcome, "ok"> | "late";

export interface Marker {
  turn: number;
  player: number;
  kind: MarkerKind;
}

/** Every problem, by turn then player. */
export function markers(turns: readonly TurnRecord[], events: readonly EventRecord[]): Marker[] {
  const found: Marker[] = [];
  for (const turn of turns) {
    turn.players.forEach((p, player) => {
      if (p.outcome !== "ok") found.push({ turn: turn.turn, player, kind: p.outcome });
    });
  }
  const late = new Set<string>();
  for (const event of events) {
    const key = `${event.turn}:${event.player}`;
    if (event.kind === "late" && !late.has(key)) {
      late.add(key);
      found.push({ turn: event.turn, player: event.player, kind: "late" });
    }
  }
  return found.sort((a, b) => a.turn - b.turn || a.player - b.player);
}

/** The turns where a problem starts: one the same player did not have on the turn before. */
export function problemStarts(found: readonly Marker[]): number[] {
  const seen = new Set(found.map((m) => `${m.turn}:${m.player}:${m.kind}`));
  const starts = new Set<number>();
  for (const m of found) {
    if (!seen.has(`${m.turn - 1}:${m.player}:${m.kind}`)) starts.add(m.turn);
  }
  return [...starts].sort((a, b) => a - b);
}

/** The first problem start after `turn`. */
export function nextProblem(starts: readonly number[], turn: number): number | null {
  return starts.find((t) => t > turn) ?? null;
}

/** The last problem start before `turn`. */
export function previousProblem(starts: readonly number[], turn: number): number | null {
  return starts.findLast((t) => t < turn) ?? null;
}
