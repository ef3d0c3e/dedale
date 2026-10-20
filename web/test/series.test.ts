import assert from "node:assert/strict";
import { test } from "node:test";

import {
  durationTick,
  forLog,
  points,
  pointsLeft,
  powersOfTen,
  problems,
  series,
  spread,
} from "../src/game/series.ts";
import { parseReplay } from "../src/validate.ts";
import { fixture } from "./load.ts";

const record = parseReplay(fixture("replay.json"));

test("series are one value per turn", () => {
  for (const metric of ["latency_us", "think_us", "half_points"] as const) {
    for (const player of [0, 1]) {
      const values = series(record.turns, player, metric);
      assert.equal(values.length, record.turns.length);
      for (const [i, v] of values.entries()) {
        assert.equal(v, record.turns[i]?.players[player]?.[metric] ?? null);
      }
    }
  }
  const silent = structuredClone(record.turns);
  const answer = silent[2]?.players[1];
  assert.ok(answer);
  answer.latency_us = null;
  assert.equal(series(silent, 1, "latency_us")[2], null, "an unanswered turn has no latency");
});

test("durations on a log axis never reach 0, and scores are in points", () => {
  assert.deepEqual(forLog([0, 0.5, 3, null, 120_000]), [1, 1, 3, null, 120_000]);
  assert.deepEqual(points([0, 1, 4, null]), [0, 0.5, 2, null]);
});

test("problems sit at their turns, nowhere else", () => {
  const turns = structuredClone(record.turns);
  const answer = turns[1]?.players[1];
  assert.ok(answer);
  answer.outcome = "timeout";
  const at = problems(turns, 1, (i) => 1000 + i);
  assert.equal(at[1], 1001);
  for (const [i, v] of at.entries()) {
    const outcome = turns[i]?.players[1]?.outcome;
    assert.equal(v, outcome === "ok" ? null : 1000 + i, `turn ${i + 1}`);
  }
  // A player the game does not have has no problems.
  assert.ok(problems(turns, 5, () => 1).every((v) => v === null));
});

test("points left fall by what each turn collected", () => {
  const total = record.maze.cells.split("").filter((c) => c === ".").length;
  const left = pointsLeft(total, record.turns);
  assert.equal(left.length, record.turns.length);
  let expected = total;
  for (const [i, turn] of record.turns.entries()) {
    expected -= turn.collected.length;
    assert.equal(left[i], expected);
  }
  assert.ok(left.every((v, i) => i === 0 || v <= (left[i - 1] ?? total)));
});

test("a log axis has a line at every power of ten in range", () => {
  assert.deepEqual(powersOfTen(1, 100_000), [1, 10, 100, 1000, 10_000, 100_000]);
  assert.deepEqual(powersOfTen(3, 999), [10, 100]);
  assert.deepEqual(powersOfTen(0, 5), [1]);
  assert.deepEqual(powersOfTen(20, 50), []);
});

test("duration ticks read without decimals", () => {
  assert.deepEqual([1, 10, 100, 1000, 100_000, 1_000_000].map(durationTick), [
    "1 µs",
    "10 µs",
    "100 µs",
    "1 ms",
    "100 ms",
    "1 s",
  ]);
});

test("spreads are computed as the game's statistics are", () => {
  const range = (n: number) => Array.from({ length: n }, (_, i) => i + 1);
  assert.deepEqual(spread(range(100)), { mean: 50, p95: 95, p99: 99 });
  assert.deepEqual(spread(range(20).reverse()), { mean: 10, p95: 19, p99: 20 });
  assert.deepEqual(spread([10, null, 2]), { mean: 6, p95: 10, p99: 10 });
  assert.deepEqual(spread([7]), { mean: 7, p95: 7, p99: 7 });
  assert.equal(spread([null, null]), undefined);
  assert.equal(spread([]), undefined);
  for (const metric of ["latency_us", "think_us"] as const) {
    for (const [player, stats] of record.stats.entries()) {
      const stored = stats[metric];
      const { mean, p95, p99 } = stored ?? {};
      assert.deepEqual(
        spread(series(record.turns, player, metric)),
        stored ? { mean, p95, p99 } : undefined,
        `${metric} of player ${player}`,
      );
    }
  }
});
