import assert from "node:assert/strict";
import { test } from "node:test";

import { parseRoute, type Route, routeHash } from "../src/router.ts";

test("routes round-trip through the hash", () => {
  const routes: Route[] = [
    { page: "home" },
    { page: "live" },
    { page: "game", path: "run 1/replay.json", turn: null },
    { page: "game", path: "cup/games/001-duo/replay.json", turn: 12 },
    { page: "tournament", path: "a#b?c/tournament.json" },
  ];
  for (const route of routes) assert.deepEqual(parseRoute(routeHash(route)), route);
  assert.equal(
    routeHash({ page: "game", path: "run 1/replay.json", turn: 3 }),
    "#/game/run%201/replay.json?turn=3",
  );
});

test("hashes from elsewhere", () => {
  assert.deepEqual(parseRoute(""), { page: "home" });
  assert.deepEqual(parseRoute("#"), { page: "home" });
  assert.deepEqual(parseRoute("#/game/x/replay.json?turn=-1"), {
    page: "game",
    path: "x/replay.json",
    turn: null,
  });
  assert.deepEqual(parseRoute("#/game/x/replay.json?turn=abc"), {
    page: "game",
    path: "x/replay.json",
    turn: null,
  });
  assert.deepEqual(parseRoute("#/game/"), { page: "unknown", hash: "#/game/" });
  assert.deepEqual(parseRoute("#/game/%zz"), { page: "unknown", hash: "#/game/%zz" });
  assert.deepEqual(parseRoute("#/elsewhere"), { page: "unknown", hash: "#/elsewhere" });
});
