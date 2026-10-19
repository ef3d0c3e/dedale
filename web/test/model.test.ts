import assert from "node:assert/strict";
import { test } from "node:test";

import { GameModel } from "../src/model.ts";
import type { GameRecord, Pos, TurnRecord } from "../src/types.ts";
import { parseReplay } from "../src/validate.ts";
import { fixture } from "./load.ts";

const record = parseReplay(fixture("replay.json"));
const states = fixture("states.json") as {
  turn: number;
  cells: string;
  players: { pos: Pos; half_points: number; last_move: string | null }[];
}[];

test("states equal the ones Rust rebuilds, at every turn", () => {
  const model = GameModel.fromRecord(record);
  assert.equal(states.length, record.turns.length + 1);
  for (const expected of states) {
    assert.equal(model.board(expected.turn), expected.cells, `turn ${expected.turn}`);
    const state = model.state(expected.turn);
    assert.equal(state.turn, expected.turn);
    assert.deepEqual(state.players, expected.players, `turn ${expected.turn}`);
    assert.equal(state.remaining, [...expected.cells].filter((c) => c === ".").length);
  }
});

test("a game grown turn by turn gives the same states", () => {
  const whole = GameModel.fromRecord(record);
  const growing = new GameModel(
    record.maze,
    record.players.map((p) => p.start),
  );
  for (const turn of record.turns) {
    growing.push(turn);
    for (let t = 0; t <= growing.length; t++) {
      assert.equal(growing.board(t), whole.board(t));
      assert.deepEqual(growing.state(t), whole.state(t));
    }
  }
});

test("collection turns and clamping", () => {
  const model = GameModel.fromRecord(record);
  record.turns.forEach((turn) => {
    for (const [x, y] of turn.collected) assert.equal(model.takenOn(x, y), turn.turn);
  });
  assert.equal(model.takenOn(0, 0), null);
  assert.deepEqual(model.state(10_000), model.state(record.turns.length));
  assert.deepEqual(model.state(-3), model.state(0));
  assert.equal(model.cell(99, 0, 0), "wall");
});

/** The board after `turn`, by applying every collection up to it. */
function rebuild(maze: GameRecord["maze"], turns: TurnRecord[], turn: number): string {
  const cells = [...maze.cells];
  for (const t of turns.slice(0, turn)) {
    for (const [x, y] of t.collected) cells[y * maze.width + x] = "_";
  }
  return cells.join("");
}

test("a long game on the largest maze stays fast", () => {
  const size = 255;
  const maze = { width: size, height: size, cells: ".".repeat(size * size) };
  // A walker sweeping the maze row by row, one cell per turn.
  const turns: TurnRecord[] = [];
  for (let t = 1; t < size * size; t++) {
    const pos: Pos = [t % size, Math.floor(t / size)];
    const player = {
      requested: "right" as const,
      applied: "right" as const,
      outcome: "ok" as const,
      latency_us: 1,
      think_us: 1,
      pos,
      gained: 2,
      half_points: 2 * t,
      log: [],
      marks: [],
      error: null,
      invalid: null,
    };
    turns.push({ turn: t, players: [player], collected: [pos] });
  }
  const begun = performance.now();
  const model = new GameModel(maze, [[0, 0]], turns);
  for (const t of [0, 1, 1000, 30_000, turns.length]) model.state(t);
  const elapsed = performance.now() - begun;
  assert.ok(elapsed < 1000, `${elapsed} ms`);
  for (const t of [0, 1, 254, 255, 20_000, turns.length]) {
    assert.equal(model.board(t), rebuild(maze, turns, t), `turn ${t}`);
  }
  assert.equal(model.state(turns.length).remaining, 1);
});
