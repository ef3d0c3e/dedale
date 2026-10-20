import assert from "node:assert/strict";
import { test } from "node:test";

import { fromRecord, type GameData } from "../src/game/data.ts";
import { exitSummary, inspect } from "../src/game/inspect.ts";
import { describeMark } from "../src/game/inspector.ts";
import type { GameRecord } from "../src/types.ts";
import { parseReplay } from "../src/validate.ts";
import { damaged, fixture, type Loose } from "./load.ts";

const replay = fixture("replay.json");
const golden = parseReplay(replay);

function data(change: (r: Loose) => void = () => {}): GameData {
  return fromRecord(parseReplay(damaged(replay, change)) as GameRecord);
}

const field = (
  panel: { fields: { label: string; value: string; tone: string }[] },
  label: string,
) => panel.fields.find((f) => f.label === label);

test("every turn of the golden replay shows what each player did", () => {
  const game = fromRecord(golden);
  for (let turn = 1; turn <= golden.turns.length; turn++) {
    const panels = inspect(game, turn);
    assert.equal(panels.length, 2);
    panels.forEach((panel, i) => {
      const p = golden.turns[turn - 1]?.players[i];
      assert.ok(p);
      assert.equal(panel.title, `${"AB"[i]} ${golden.players[i]?.name}`);
      assert.equal(field(panel, "outcome")?.tone, p.outcome === "ok" ? "ok" : "bad");
      assert.equal(field(panel, "position")?.value, `(${p.pos[0]}, ${p.pos[1]})`);
      assert.deepEqual(panel.log, p.log);
      assert.deepEqual(panel.marks, p.marks);
      assert.equal(panel.error, p.error);
      assert.equal(panel.invalid, p.invalid);
      const events = golden.events.filter((e) => e.turn === turn && e.player === i);
      assert.equal(panel.events.length, events.length);
      // The exit shows once the game is over.
      assert.equal(panel.exit !== null, turn === golden.turns.length);
    });
  }
});

test("a turn's details", () => {
  const game = data((r) => {
    const p = r.turns[0].players[0];
    Object.assign(p, {
      requested: "up",
      applied: "stay",
      outcome: "illegal",
      latency_us: 1500,
      think_us: 900,
      gained: 0,
    });
    Object.assign(r.turns[0].players[1], {
      requested: null,
      outcome: "error",
      error: { message: "ZeroDivisionError: division by zero", traceback: "Traceback…\n  line 3" },
    });
    Object.assign(r.turns[1].players[1], { outcome: "invalid", invalid: 'unknown move "north"' });
    r.events = [
      { turn: 2, player: 1, kind: "late", for_turn: 1 },
      { turn: 2, player: 1, kind: "ignored", reason: "unexpected ready" },
    ];
  });
  const [a, b] = inspect(game, 1);
  assert.ok(a && b);
  assert.equal(field(a, "move")?.value, "up, stayed");
  assert.deepEqual(field(a, "outcome"), { label: "outcome", value: "illegal", tone: "bad" });
  assert.equal(field(a, "latency")?.value, "1.5 ms of 100 ms");
  assert.equal(field(a, "in move()")?.value, "900 µs");
  assert.equal(field(b, "move")?.value, "nothing, stayed");
  assert.equal(b.error?.traceback, "Traceback…\n  line 3");
  const [, second] = inspect(game, 2);
  assert.equal(second?.invalid, 'unknown move "north"');
  assert.deepEqual(second?.events, ["late answer to turn 1", "ignored: unexpected ready"]);
});

test("turn 0 shows the startup", () => {
  const cases: [Loose, string, string][] = [
    [{ outcome: "ok", startup_us: 41210, init_us: 39870 }, "ready", "ok"],
    [{ outcome: "late", startup_us: 2_500_000, init_us: 2_400_000 }, "ready late", "bad"],
    [
      { outcome: "init_error", error: { message: "ImportError", traceback: null } },
      "init() raised",
      "bad",
    ],
    [{ outcome: "timeout", startup_us: null, init_us: null }, "not ready in time", "bad"],
    [{ outcome: "crashed" }, "crashed", "bad"],
  ];
  for (const [startup, words, tone] of cases) {
    const game = data((r) => Object.assign(r.players[0].startup, startup));
    const [a] = inspect(game, 0);
    assert.ok(a);
    assert.deepEqual(field(a, "startup"), { label: "startup", value: words, tone });
    assert.equal(a.error?.message, startup.error?.message);
  }
  const [a] = inspect(
    data((r) => Object.assign(r.players[0].startup, { startup_us: 41210 })),
    0,
  );
  assert.equal(field(a ?? { fields: [] }, "ready after")?.value, "41.2 ms of 500 ms");
  // A live game whose bot has not reported yet.
  const live = { ...fromRecord(golden), startups: [null, null] };
  assert.equal(field(inspect(live, 0)[0] ?? { fields: [] }, "startup")?.value, "starting…");
});

test("exits", () => {
  const exit = { code: null, signal: null, killed: false, stderr_tail: "" };
  assert.equal(exitSummary({ ...exit, code: 0 }), "exited normally");
  assert.equal(exitSummary({ ...exit, code: 3 }), "exited with code 3");
  assert.equal(exitSummary({ ...exit, signal: 11 }), "killed by signal 11");
  assert.equal(exitSummary({ ...exit, killed: true, signal: 9 }), "killed by Dedale (signal 9)");
  assert.equal(exitSummary({ ...exit, killed: true }), "killed by Dedale");
  assert.equal(exitSummary(exit), "exit unknown");
  const game = data((r) => Object.assign(r.players[1].exit, { code: 3, stderr_tail: "boom\n" }));
  const b = inspect(game, golden.turns.length)[1];
  assert.deepEqual(b?.exit, { summary: "exited with code 3", stderr: "boom\n" });
});

test("hostile text stays text", () => {
  const hostile = '<img src=x onerror="alert(1)"><script>alert(2)</script>&amp;';
  const game = data((r) => {
    r.turns[0].players[0].log = [hostile];
    r.turns[0].players[0].marks = [
      { cells: [[0, 0]], label: hostile, color: "red;background:url(x)" },
    ];
  });
  const [a] = inspect(game, 1);
  assert.deepEqual(a?.log, [hostile], "passed on verbatim, for the page to show as text");
  assert.equal(a?.marks[0]?.label, hostile);
  // Only palette names reach the page's styles.
  assert.equal(a?.marks[0]?.color, null);
});

test("marks are described with their kind", () => {
  assert.equal(
    describeMark({
      cells: [
        [0, 0],
        [1, 0],
        [2, 0],
      ],
      label: "route",
      color: "teal",
      kind: "path",
    }),
    "route: a path through 3 cells",
  );
  assert.equal(describeMark({ cells: [[4, 4]], label: null, color: null }), "unlabelled: 1 cell");
});

test("sandboxed games say so at startup, with each bot's CPU", () => {
  assert.equal(field(inspect(data(), 0)[0] ?? { fields: [] }, "sandbox"), undefined);
  const sandboxed = data((r) => {
    r.config.sandbox = true;
    r.players[0].cpu = 11;
  });
  assert.equal(sandboxed.config.sandbox, true);
  const [a, b] = inspect(sandboxed, 0);
  assert.equal(field(a ?? { fields: [] }, "sandbox")?.value, "on, CPU 11");
  assert.equal(field(b ?? { fields: [] }, "sandbox")?.value, "on");
});
