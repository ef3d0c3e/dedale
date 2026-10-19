import assert from "node:assert/strict";
import { test } from "node:test";

import {
  columnRange,
  downsample,
  linearScale,
  logScale,
  series,
  turnAt,
  turnX,
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

test("downsampling keeps every spike and the bounds", () => {
  const values: (number | null)[] = Array.from({ length: 10_000 }, (_, i) =>
    i % 2 === 0 ? 10 : 20,
  );
  values[4321] = 99_999;
  values[777] = 1;
  values[5000] = null;
  const buckets = downsample(values, 300);
  assert.equal(buckets.length, 300);
  const all = buckets.flatMap((b) => (b ? [b.min, b.max] : []));
  assert.equal(Math.max(...all), 99_999);
  assert.equal(Math.min(...all), 1);
  // The spike lands in the column covering its turn.
  const column = buckets.findIndex((b) => b?.max === 99_999);
  const [from, to] = columnRange(column, values.length, 300);
  assert.ok(from <= 4321 && 4321 < to);
  // Few values: one bucket each, unchanged.
  assert.deepEqual(downsample([3, null, 5], 100), [{ min: 3, max: 3 }, null, { min: 5, max: 5 }]);
  assert.deepEqual(downsample([], 100), []);
  assert.deepEqual(downsample([null, null, null, null], 2), [null, null]);
});

test("columns cover every turn exactly once", () => {
  for (const [n, columns] of [
    [10, 3],
    [1000, 7],
    [5, 5],
    [3, 10],
  ] as const) {
    const count = Math.min(n, columns);
    const covered: number[] = [];
    for (let c = 0; c < count; c++) {
      const [from, to] = columnRange(c, n, count);
      for (let i = from; i < to; i++) covered.push(i);
    }
    assert.deepEqual(
      covered,
      Array.from({ length: n }, (_, i) => i),
      `${n} in ${columns}`,
    );
  }
});

test("scales", () => {
  const linear = linearScale(37);
  assert.deepEqual(linear.ticks, [0, 10, 20, 30, 40, 50]);
  assert.equal(linear.at(0), 0);
  assert.equal(linear.at(50), 1);
  assert.equal(linear.at(500), 1, "clamped");
  assert.deepEqual(linearScale(0).ticks, [0, 0.25, 0.5, 0.75, 1]);
  assert.deepEqual(linearScale(150).ticks, [0, 50, 100, 150, 200]);
  const log = logScale(100_000);
  assert.deepEqual(log.ticks, [1, 10, 100, 1000, 10_000, 100_000]);
  assert.equal(log.at(1), 0);
  assert.equal(log.at(100_000), 1);
  assert.ok(Math.abs(log.at(1000) - 0.6) < 1e-9);
  assert.equal(log.at(0), 0, "zero sits at the bottom");
  assert.deepEqual(logScale(150_000).ticks.at(-1), 1_000_000);
});

test("hit testing maps pixels to turns and back", () => {
  assert.equal(turnAt(0, 100, 10), 1);
  assert.equal(turnAt(99.9, 100, 10), 10);
  assert.equal(turnAt(150, 100, 10), 10, "clamped");
  assert.equal(turnAt(-5, 100, 10), 1, "clamped");
  for (const [width, turns] of [
    [100, 10],
    [640, 3844],
    [500, 7],
  ] as const) {
    for (let turn = 1; turn <= turns; turn += Math.ceil(turns / 50)) {
      assert.equal(turnAt(turnX(turn, width, turns), width, turns), turn);
    }
  }
});
