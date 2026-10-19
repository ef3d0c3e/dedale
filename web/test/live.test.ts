import assert from "node:assert/strict";
import { test } from "node:test";

import { LiveState, record } from "../src/live.ts";
import type { LiveEvent, LiveSnapshot } from "../src/types.ts";
import { parseLiveEvent, parseReplay } from "../src/validate.ts";
import { fixture } from "./load.ts";

const events: LiveEvent[] = (fixture("events.json") as { name: string; data: unknown }[]).map((e) =>
  parseLiveEvent(e.name, e.data),
);
const replay = parseReplay(fixture("replay.json"));

test("a streamed game assembles into the written replay", () => {
  const state = new LiveState();
  for (const event of events) {
    state.apply(event);
    if (event.name === "turn") {
      const game = state.game(1);
      assert.ok(game);
      assert.equal(record(game), null, "not over yet");
    }
  }
  assert.equal(state.session?.phase, "finished");
  assert.equal(state.session?.file, "replay.json");
  const game = state.game(1);
  assert.ok(game);
  assert.deepEqual(
    game.startups,
    replay.players.map((p) => p.startup),
  );
  assert.deepEqual(record(game), replay);
});

test("a page joining late catches up from a snapshot", () => {
  for (let cut = 3; cut < events.length; cut++) {
    const early = new LiveState();
    for (const event of events.slice(0, cut)) early.apply(event);
    // What the server sends a page connecting then.
    assert.ok(early.session);
    const snapshot: LiveSnapshot = structuredClone({
      session: early.session,
      pace: early.pace,
      games: early.games,
      standings: early.standings,
    });
    const late = new LiveState();
    late.apply({ name: "snapshot", data: snapshot });
    // A turn already in the snapshot may arrive again: it is not doubled.
    const replayed = events
      .slice(cut - 1)
      .filter((e) => e.name === "turn" || e !== events[cut - 1]);
    for (const event of replayed) late.apply(event);
    const game = late.game(1);
    assert.ok(game, `cut ${cut}`);
    assert.deepEqual(record(game), replay, `cut ${cut}`);
  }
});

test("pace, standings and errors", () => {
  const state = new LiveState();
  const first = events[0];
  assert.ok(first);
  state.apply(first);
  state.apply({ name: "pace", data: { paused: true, delay_ms: 30 } });
  assert.deepEqual(state.pace, { paused: true, delay_ms: 30 });
  state.apply({ name: "standings", data: { standings: [] } });
  assert.deepEqual(state.standings, []);
  state.apply({ name: "error", data: { message: "cannot start bot" } });
  assert.equal(state.session?.phase, "failed");
  assert.equal(state.session?.error, "cannot start bot");
  assert.equal(state.current, null);
});
