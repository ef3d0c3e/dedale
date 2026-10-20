import assert from "node:assert/strict";
import { test } from "node:test";

import {
  FormatError,
  parseListing,
  parseLiveEvent,
  parseReplay,
  parseTournament,
  session,
} from "../src/validate.ts";
import { damaged, fixture, type Loose } from "./load.ts";

const replay = fixture("replay.json");

test("the golden replay and tournament load unchanged", () => {
  assert.deepEqual(parseReplay(replay), replay);
  const tournament = fixture("tournament.json");
  assert.deepEqual(parseTournament(tournament), tournament);
});

test("damaged replays are refused with the field at fault", () => {
  const cases: [(r: Loose) => void, RegExp][] = [
    [(r) => (r.format = 2), /^unsupported replay format 2 \(this app reads format 1\)$/],
    [(r) => delete r.format, /^format: expected a whole number, found undefined$/],
    [(r) => delete r.result, /^result: expected an object, found undefined$/],
    [
      (r) => (r.turns[3].players[1].pos = [1]),
      /^turns\[3\]\.players\[1\]\.pos: expected a position/,
    ],
    [
      (r) => (r.turns[0].players[0].outcome = "lucky"),
      /turns\[0\]\.players\[0\]\.outcome: expected "ok" or/,
    ],
    [(r) => (r.turns[2].turn = 9), /^turns\[2\]\.turn: expected 3, found 9$/],
    [
      (r) => (r.turns[2].players[0].pos = [5, 0]),
      /^turns\[2\]\.players\[0\]\.pos: expected a position inside/,
    ],
    [(r) => r.turns[1].players.pop(), /^turns\[1\]\.players: expected 2 players/],
    [(r) => (r.maze.cells = "#"), /^maze\.cells: expected 5×5 cells/],
    [(r) => (r.maze.cells = "x".repeat(25)), /^maze\.cells: expected only #, \. and _/],
    [
      (r) => (r.players[0].startup.init_us = -1),
      /^players\[0\]\.startup\.init_us: expected a whole number/,
    ],
    [(r) => (r.events = [{ turn: 1, player: 0, kind: "late" }]), /^events\[0\]\.for_turn/],
    [(r) => (r.result.half_points = [1]), /^result\.half_points: expected 2 scores/],
  ];
  for (const [change, expected] of cases) {
    assert.throws(
      () => parseReplay(damaged(replay, change)),
      (e: Error) => {
        assert.ok(e instanceof FormatError, String(e));
        assert.match(e.message, expected);
        return true;
      },
    );
  }
  assert.throws(() => parseReplay(null), { message: "the file: expected an object, found null" });
  assert.throws(() => parseReplay([]), { message: "the file: expected an object, found an array" });
});

test("marks keep palette colours and paths, and read anything else as Dedale does", () => {
  const marked = damaged(replay, (r) => {
    r.turns[0].players[0].marks = [
      {
        cells: [
          [0, 0],
          [1, 0],
        ],
        label: "route",
        color: "teal",
        kind: "path",
      },
      { cells: [[2, 0]], label: null, color: "magenta", kind: "circle" },
      { cells: [[3, 0]], label: null, color: "#ff0000" },
      { cells: [[4, 0]], label: null },
    ];
  });
  assert.deepEqual(parseReplay(marked).turns[0]?.players[0]?.marks, [
    {
      cells: [
        [0, 0],
        [1, 0],
      ],
      label: "route",
      color: "teal",
      kind: "path",
    },
    { cells: [[2, 0]], label: null, color: null },
    { cells: [[3, 0]], label: null, color: null },
    { cells: [[4, 0]], label: null, color: null },
  ]);
});

test("older files say nothing about their bots", () => {
  const nothing = { name: null, author: null, description: null };
  const old = damaged(replay, (r) => {
    for (const player of r.players) delete player.about;
  });
  assert.deepEqual(
    parseReplay(old).players.map((p) => p.about),
    [nothing, nothing],
  );
  const tournament = damaged(fixture("tournament.json"), (t) => {
    for (const bot of t.bots) delete bot.about;
  });
  assert.deepEqual(
    parseTournament(tournament).bots.map((b) => b.about),
    [nothing, nothing],
  );
  assert.equal(parseReplay(replay).players[0]?.about.author, "Ada");
});

test("damaged tournaments are refused", () => {
  const tournament = fixture("tournament.json");
  assert.throws(() => parseTournament(damaged(tournament, (t) => (t.format = 9))), /format 9/);
  assert.throws(() => parseTournament(damaged(tournament, (t) => (t.games[1].seating = "trio"))), {
    message: 'games[1].seating: expected "solo" or "duo" or "duo_swapped", found "trio"',
  });
});

test("listings and sessions", () => {
  const listing = {
    entries: [
      {
        kind: "game",
        path: "a/replay.json",
        modified: 3,
        error: null,
        code: null,
        seating: "solo",
        bots: ["x"],
        result: null,
      },
      {
        kind: "tournament",
        path: "t/tournament.json",
        modified: 2,
        error: "broken",
        mode: null,
        bots: [],
        leaders: [],
        games: [],
      },
    ],
    truncated: false,
  };
  assert.deepEqual(parseListing(listing), listing);
  assert.throws(
    () => parseListing({ entries: [{ kind: "film" }], truncated: false }),
    /entries\[0\]\.kind/,
  );
  assert.deepEqual(session({ mode: "browse", title: "out" }, "session"), {
    mode: "browse",
    title: "out",
  });
  assert.throws(() => session({ mode: "run", title: "x" }, "session"), {
    message: /^session\.bots: expected an array/,
  });
});

test("every streamed event parses", () => {
  const events = fixture("events.json") as { name: string; data: unknown }[];
  assert.deepEqual(
    events.map((e) => parseLiveEvent(e.name, e.data).name),
    events.map((e) => e.name),
  );
  assert.throws(() => parseLiveEvent("party", {}), /unknown event "party"/);
  assert.throws(() => parseLiveEvent("pace", { paused: 1, delay_ms: 0 }), {
    message: /^pace\.paused: expected true or false/,
  });
});
