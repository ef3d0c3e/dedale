import assert from "node:assert/strict";
import { test } from "node:test";

import { highest, strength, takenBy, Visits } from "../src/game/heat.ts";
import { GameModel } from "../src/model.ts";
import { parseReplay } from "../src/validate.ts";
import { fixture } from "./load.ts";

const record = parseReplay(fixture("replay.json"));
const model = GameModel.fromRecord(record);

/** Visits counted the slow way: every state from 0 to `turn`. */
function naive(player: number, turn: number): number[] {
  const counts = new Array(record.maze.width * record.maze.height).fill(0);
  for (let state = 0; state <= turn; state++) {
    const [x, y] =
      state === 0
        ? (record.players[player]?.start ?? [0, 0])
        : (record.turns[state - 1]?.players[player]?.pos ?? [0, 0]);
    counts[y * record.maze.width + x]++;
  }
  return counts;
}

test("visits up to any turn, scrubbing either way", () => {
  for (const player of [0, 1]) {
    const visits = new Visits(model, player);
    const order = [0, 5, 12, 3, 3, 11, 0, 12, 7, 99];
    for (const turn of order) {
      const expected = naive(player, Math.min(turn, record.turns.length));
      assert.deepEqual([...visits.at(turn)], expected, `player ${player}, turn ${turn}`);
    }
  }
  // Hand-checked: A starts at (0, 0) and leaves it on turn 1.
  const a = new Visits(model, 0);
  assert.equal(a.at(0)[0], 1);
  assert.equal(highest(a.at(0)), 1);
  assert.equal(
    a.at(record.turns.length).reduce((sum, c) => sum + c, 0),
    record.turns.length + 1,
  );
});

test("collection order", () => {
  for (let turn = 0; turn <= record.turns.length; turn++) {
    const taken = takenBy(model, turn);
    const expected = record.turns
      .slice(0, turn)
      .flatMap((t) => t.collected.map(([x, y]) => `${x},${y}@${t.turn}`));
    assert.deepEqual(
      taken.map((t) => `${t.x},${t.y}@${t.turn}`),
      expected,
      `turn ${turn}`,
    );
  }
  assert.deepEqual(takenBy(model, 0), []);
});

test("strength grows with the count", () => {
  assert.equal(strength(0, 0), 0);
  assert.equal(strength(1, 1), 0.85);
  assert.ok(strength(1, 10) < strength(5, 10));
  assert.equal(strength(20, 10), 0.85, "capped");
});

test("visits on the largest maze stay fast", () => {
  const size = 255;
  const maze = { width: size, height: size, cells: "_".repeat(size * size) };
  const turns = Array.from({ length: size * size - 1 }, (_, i) => {
    const t = i + 1;
    return {
      turn: t,
      collected: [],
      players: [
        {
          requested: "right" as const,
          applied: "right" as const,
          outcome: "ok" as const,
          latency_us: 1,
          think_us: 1,
          pos: [t % size, Math.floor(t / size)] as const,
          gained: 0,
          half_points: 0,
          log: [],
          marks: [],
          error: null,
          invalid: null,
        },
      ],
    };
  });
  const big = new GameModel(maze, [[0, 0]], turns);
  const visits = new Visits(big, 0);
  const begun = performance.now();
  for (const turn of [turns.length, 0, 30_000, turns.length, 10]) visits.at(turn);
  const elapsed = performance.now() - begun;
  assert.ok(elapsed < 1000, `${elapsed} ms`);
  assert.equal(highest(visits.at(turns.length)), 1);
  assert.equal(
    visits.at(10).reduce((sum, c) => sum + c, 0),
    11,
  );
});
