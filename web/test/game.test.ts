import assert from "node:assert/strict";
import { test } from "node:test";

import { describeResult, formatUs, score } from "../src/format.ts";
import { cellAt, cellCentre, cellOrigin, fit } from "../src/game/layout.ts";
import { advance, step } from "../src/game/playback.ts";
import { markers, nextProblem, previousProblem, problemStarts } from "../src/game/timeline.ts";
import { GameModel } from "../src/model.ts";
import type { EventRecord, Outcome, TurnRecord } from "../src/types.ts";
import { parseReplay } from "../src/validate.ts";
import { fixture } from "./load.ts";

const record = parseReplay(fixture("replay.json"));

test("cells fit the space in whole pixels", () => {
  assert.deepEqual(fit(31, 31, 640, 480), {
    cols: 31,
    rows: 31,
    cell: 15,
    width: 465,
    height: 465,
  });
  assert.deepEqual(fit(255, 255, 300, 300).cell, 1);
  assert.deepEqual(fit(5, 5, 2, 2).cell, 1, "never below a pixel");
  assert.deepEqual(fit(5, 5, 1000, 1000).cell, 40, "never huge");
  const layout = fit(11, 7, 110, 70);
  assert.deepEqual(cellAt(layout, 0, 0), [0, 0]);
  assert.deepEqual(cellAt(layout, 109.9, 69.9), [10, 6]);
  assert.equal(cellAt(layout, 110, 10), null);
  assert.equal(cellAt(layout, -1, 10), null);
  assert.deepEqual(cellOrigin(layout, 3, 2), [30, 20]);
  // Paths join cell centres.
  assert.deepEqual(cellCentre(layout, 3, 2), [35, 25]);
  assert.deepEqual(cellAt(layout, ...cellCentre(layout, 10, 6)), [10, 6]);
  for (const [x, y] of [
    [0, 0],
    [4, 5],
    [10, 6],
  ] as const) {
    const [px, py] = cellOrigin(layout, x, y);
    assert.deepEqual(cellAt(layout, px + layout.cell / 2, py + layout.cell / 2), [x, y]);
  }
});

/** A turn where each player had `outcomes[i]`. */
function turnWith(turn: number, outcomes: Outcome[]): TurnRecord {
  return {
    turn,
    collected: [],
    players: outcomes.map((outcome) => ({
      requested: null,
      applied: "stay",
      outcome,
      latency_us: null,
      think_us: null,
      pos: [0, 0],
      gained: 0,
      half_points: 0,
      log: [],
      marks: [],
      error: null,
      invalid: null,
    })),
  };
}

test("problems are marked per player and navigated by where they start", () => {
  const turns = [
    turnWith(1, ["ok", "ok"]),
    turnWith(2, ["timeout", "ok"]),
    turnWith(3, ["ok", "crashed"]),
    turnWith(4, ["ok", "crashed"]),
    turnWith(5, ["illegal", "crashed"]),
    turnWith(6, ["ok", "crashed"]),
  ];
  const events: EventRecord[] = [
    { turn: 3, player: 0, kind: "late", for_turn: 2 },
    { turn: 3, player: 0, kind: "late", for_turn: 1 },
    { turn: 4, player: 1, kind: "ignored", reason: "noise" },
  ];
  const found = markers(turns, events);
  assert.deepEqual(
    found.map((m) => `${m.turn}${"AB"[m.player]}:${m.kind}`),
    ["2A:timeout", "3A:late", "3B:crashed", "4B:crashed", "5A:illegal", "5B:crashed", "6B:crashed"],
  );
  // The crash counts once, where it starts.
  const starts = problemStarts(found);
  assert.deepEqual(starts, [2, 3, 5]);
  assert.equal(nextProblem(starts, 0), 2);
  assert.equal(nextProblem(starts, 3), 5);
  assert.equal(nextProblem(starts, 5), null);
  assert.equal(previousProblem(starts, 5), 3);
  assert.equal(previousProblem(starts, 2), null);
});

test("the golden replay's problems", () => {
  const found = markers(record.turns, record.events);
  for (const m of found) {
    const turn = record.turns[m.turn - 1];
    if (m.kind === "late") {
      assert.ok(
        record.events.some((e) => e.kind === "late" && e.turn === m.turn && e.player === m.player),
      );
    } else {
      assert.equal(turn?.players[m.player]?.outcome, m.kind);
    }
  }
  assert.ok(found.length > 0, "the golden replay has problems to show");
});

test("playback steps at the chosen speed", () => {
  // 10 turns a second: a turn per 100 ms, the rest carried over.
  assert.deepEqual(advance(0, 0, 250, 10, 50), { turn: 2, carry: 50, done: false });
  assert.deepEqual(advance(2, 50, 60, 10, 50), { turn: 3, carry: 10, done: false });
  assert.deepEqual(advance(48, 0, 1000, 10, 50), { turn: 50, carry: 0, done: true });
  assert.deepEqual(advance(5, 0, -20, 10, 50), { turn: 5, carry: 0, done: false });
  assert.equal(advance(0, 0, 16, 1000, 1000).turn, 16);
  assert.equal(step(3, -5, 10), 0);
  assert.equal(step(3, 5, 6), 6);
});

test("numbers read as in the terminal", () => {
  assert.equal(formatUs(null), "-");
  assert.equal(formatUs(850), "850 µs");
  assert.equal(formatUs(12_345), "12.3 ms");
  assert.equal(formatUs(1_050_000), "1.05 s");
  assert.equal(score(7), "3.5");
  assert.equal(score(8), "4");
  const names = ["greedy", "random"];
  assert.equal(
    describeResult(
      { reason: "cleared", turns: 102, half_points: [120, 24], winner: 0, cleared_turn: 102 },
      "duo",
      names,
    ),
    "cleared on turn 102: A greedy wins, 60 to 12",
  );
  assert.equal(
    describeResult(
      { reason: "turn_limit", turns: 12, half_points: [5, 5], winner: null, cleared_turn: null },
      "duo",
      names,
    ),
    "turn limit reached after 12 turns: draw, 2.5 to 2.5",
  );
  assert.equal(
    describeResult(
      { reason: "abandoned", turns: 3, half_points: [2], winner: null, cleared_turn: null },
      "solo",
      names,
    ),
    "abandoned on turn 3: every bot crashed, 1 points",
  );
});

test("each point taken is credited to who stood on it", () => {
  const model = GameModel.fromRecord(record);
  assert.equal(model.taken.length, model.points.length - model.state(model.length).remaining);
  for (const taken of model.taken) {
    const turn = record.turns[taken.turn - 1];
    assert.ok(turn);
    const on = turn.players.map((p) => p.pos[0] === taken.x && p.pos[1] === taken.y);
    assert.equal(taken.by, (on[0] ? 1 : 0) | (on[1] ? 2 : 0));
    assert.notEqual(taken.by, 0);
  }
});
