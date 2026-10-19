import assert from "node:assert/strict";
import { test } from "node:test";

import { fromLive, fromRecord, sync } from "../src/game/data.ts";
import { DELAYS, LiveController, nearestDelay, paceLabel } from "../src/live/controller.ts";
import { record } from "../src/live.ts";
import type { LiveEvent, LiveSnapshot } from "../src/types.ts";
import { parseLiveEvent, parseReplay } from "../src/validate.ts";
import { fixture } from "./load.ts";

const events: LiveEvent[] = (fixture("events.json") as { name: string; data: unknown }[]).map((e) =>
  parseLiveEvent(e.name, e.data),
);
const replay = parseReplay(fixture("replay.json"));

test("the preview comes before Start", () => {
  const controller = new LiveController();
  const [snapshot] = events;
  assert.ok(snapshot);
  controller.apply(snapshot);
  assert.ok(controller.waiting);
  assert.equal(controller.game, null);
  assert.deepEqual(controller.state.session?.start?.maze, replay.maze);
  assert.equal(
    controller.state.session?.settings,
    "duo, 500 ms startup, 100 ms per move, 12 turns at most",
  );
});

test("following the newest turn, scrubbing back, resuming", () => {
  const controller = new LiveController();
  let data = null as ReturnType<typeof fromLive> | null;
  let shown = 0;
  for (const event of events) {
    controller.apply(event);
    const game = controller.game;
    if (!game) continue;
    if (!data) data = fromLive(game);
    else sync(data, game);
    const target = controller.target(data.model.length);
    if (target !== null) shown = target;
    if (event.name === "turn" && event.data.turn.turn === 5) {
      // The user looks back at turn 2: following stops.
      shown = 2;
      controller.userMoved(shown, data.model.length);
    }
  }
  assert.ok(data);
  assert.equal(shown, 2, "not dragged along once scrubbed back");
  assert.equal(controller.following, false);
  assert.equal(controller.resume(data.model.length), replay.turns.length);
  assert.ok(controller.following);
  // Going to the newest turn by hand follows again.
  controller.userMoved(3, 12);
  controller.userMoved(12, 12);
  assert.ok(controller.following);
  // The streamed game is the written one.
  const game = controller.game;
  assert.ok(game);
  assert.deepEqual(record(game), replay);
  assert.deepEqual(data.model.turns, replay.turns);
  assert.deepEqual(data.end?.result, replay.result);
});

test("a reconnection rebuilds from the snapshot without losing the position", () => {
  const cut = events.findIndex((e) => e.name === "turn" && e.data.turn.turn === 4);
  const controller = new LiveController();
  for (const event of events.slice(0, cut + 1)) controller.apply(event);
  const game = controller.game;
  assert.ok(game);
  const data = fromLive(game);
  controller.userMoved(2, data.model.length);

  // Disconnected through turn 8: the server's snapshot then.
  const later = new LiveController();
  const resumeAt = events.findIndex((e) => e.name === "turn" && e.data.turn.turn === 8);
  for (const event of events.slice(0, resumeAt + 1)) later.apply(event);
  const session = later.state.session;
  assert.ok(session);
  const snapshot: LiveSnapshot = structuredClone({
    session,
    pace: later.state.pace,
    games: later.state.games,
    standings: later.state.standings,
  });
  controller.apply({ name: "snapshot", data: snapshot });
  const rebuilt = controller.game;
  assert.ok(rebuilt);
  sync(data, rebuilt);
  assert.equal(data.model.length, 8);
  assert.deepEqual(data.model.turns, replay.turns.slice(0, 8));
  assert.equal(controller.following, false, "still where the user was");
  for (const event of events.slice(resumeAt + 1)) controller.apply(event);
  const after = controller.game;
  assert.ok(after);
  sync(data, after);
  assert.deepEqual(data.model.turns, replay.turns);
});

test("a game that ended before the page connected comes from its replay", () => {
  const controller = new LiveController();
  const finished = new LiveController();
  for (const event of events) finished.apply(event);
  const session = finished.state.session;
  assert.ok(session);
  const games = structuredClone(finished.state.games).map((g) => ({ ...g, turns: [] }));
  controller.apply({
    name: "snapshot",
    data: { session, pace: finished.state.pace, games, standings: null },
  });
  const game = controller.game;
  assert.ok(game);
  assert.ok(controller.needsReplay(game));
  assert.equal(game.end?.replay, "replay.json");
  assert.deepEqual(fromRecord(replay).model.turns, replay.turns);
  const streamed = finished.game;
  assert.ok(streamed);
  assert.equal(finished.needsReplay(streamed), false);
});

test("pace choices", () => {
  assert.equal(nearestDelay(0), 0);
  assert.equal(nearestDelay(60), 50);
  assert.equal(nearestDelay(5000), 2000);
  assert.ok(DELAYS.every((d) => nearestDelay(d) === d));
  assert.equal(paceLabel({ paused: true, delay_ms: 100 }), "paused");
  assert.equal(paceLabel({ paused: false, delay_ms: 0 }), "full speed");
  assert.equal(paceLabel({ paused: false, delay_ms: 250 }), "250 ms between turns");
});
