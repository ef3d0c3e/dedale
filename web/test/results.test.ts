import assert from "node:assert/strict";
import { test } from "node:test";

import {
  dirOf,
  type Entry,
  filterEntries,
  gameOutcome,
  leaderboard,
  players,
  replayPath,
  settings,
  sortEntries,
  tournamentOutcome,
} from "../src/results.ts";
import type { GameSummary, TournamentSummary } from "../src/types.ts";
import { parseReplay, parseTournament } from "../src/validate.ts";
import { fixture } from "./load.ts";

const record = parseReplay(fixture("replay.json"));
const report = parseTournament(fixture("tournament.json"));

function game(
  path: string,
  modified: number,
  bots: string[],
  extra: Partial<GameSummary> = {},
): GameSummary {
  return {
    kind: "game",
    path,
    modified,
    error: null,
    code: null,
    seating: "duo",
    bots,
    result: record.result,
    ...extra,
  };
}

function cup(
  path: string,
  modified: number,
  bots: string[],
  extra: Partial<TournamentSummary> = {},
): TournamentSummary {
  return {
    kind: "tournament",
    path,
    modified,
    error: null,
    mode: "duo",
    bots,
    leaders: [],
    games: [],
    ...extra,
  };
}

const entries: Entry[] = [
  game("b/replay.json", 100, ["Greedy", "random"]),
  cup("cup/tournament.json", 300, ["lap", "sitter"]),
  game("a/replay.json", 100, ["greedy"]),
  game("broken/replay.json", 200, [], {
    error: "expected value at line 1",
    result: null,
    seating: null,
  }),
];

test("newest first, then by path", () => {
  assert.deepEqual(
    sortEntries(entries).map((e) => e.path),
    ["cup/tournament.json", "broken/replay.json", "a/replay.json", "b/replay.json"],
  );
});

test("filtering by bot name, in any case", () => {
  assert.deepEqual(
    filterEntries(entries, "GREEDY").map((e) => e.path),
    ["b/replay.json", "a/replay.json"],
  );
  assert.deepEqual(
    filterEntries(entries, " sit ").map((e) => e.path),
    ["cup/tournament.json"],
  );
  assert.equal(filterEntries(entries, "").length, 4);
  assert.equal(filterEntries(entries, "nobody").length, 0);
});

test("outcomes, broken files included", () => {
  assert.equal(
    gameOutcome(game("x", 0, ["lap", "flaky"])),
    "turn limit reached after 12 turns: A lap wins, 9 to 2",
  );
  assert.equal(gameOutcome(entries[3] as GameSummary), "expected value at line 1");
  assert.equal(gameOutcome(game("x", 0, ["a"], { result: null })), "-");
  assert.equal(
    tournamentOutcome(cup("c", 0, [], { error: "unsupported format 9" })),
    "unsupported format 9",
  );
  assert.equal(
    tournamentOutcome(cup("c", 0, [], { leaders: ["lap"], games: [game("g", 0, [])] })),
    "1 game, won by lap",
  );
  assert.equal(
    tournamentOutcome(cup("c", 0, [], { leaders: ["a", "b"] })),
    "0 games, tied first a, b",
  );
});

test("leaderboards for both modes", () => {
  const duo = leaderboard("duo", report.leaderboard);
  assert.equal(duo.headers[2], "points");
  assert.equal(duo.rows.length, 2);
  const [first] = report.leaderboard;
  assert.ok(first);
  assert.deepEqual(duo.rows[0]?.slice(0, 7), [
    String(first.rank),
    first.name,
    String(first.match_points),
    String(first.wins),
    String(first.draws),
    String(first.losses),
    String(first.half_points / 2),
  ]);
  const solo = leaderboard("solo", [{ ...first, cleared: 3, games: 4, turns_to_clear: 812 }]);
  assert.deepEqual(solo.rows[0]?.slice(2, 4), ["3/4", "812"]);
  assert.ok(solo.numeric.every((c) => c < solo.headers.length));
});

test("paths and settings", () => {
  assert.equal(dirOf("cup/tournament.json"), "cup");
  assert.equal(dirOf("tournament.json"), "");
  assert.equal(
    replayPath("out/cup/tournament.json", "games/001/replay.json"),
    "out/cup/games/001/replay.json",
  );
  assert.equal(replayPath("tournament.json", "games/001/replay.json"), "games/001/replay.json");
  assert.equal(
    settings(report),
    "5×5 mazes, 0% loops, 500 ms startup, 100 ms per move, 12 turns at most, master seed 0000000000000001",
  );
  assert.equal(players([0, 1], ["lap", "flaky"]), "A lap, B flaky");
});
