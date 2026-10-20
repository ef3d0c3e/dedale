// The whole chain in a real browser: the `dedale` binary
// serving the app, headless Chromium running it.

import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { after, before, test } from "node:test";
import { fileURLToPath } from "node:url";

import { Browser, type Page } from "./cdp.ts";
import { binary, Dedale } from "./dedale.ts";

const fixtures = fileURLToPath(new URL("../../test/fixtures/", import.meta.url));
const golden = readFileSync(join(fixtures, "replay.json"), "utf8");
const RING = "_....\n.###.\n.###.\n.###.\n...._\n";
const HOSTILE = "<img src=x onerror=\"document.title='pwned'\">";

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

function put(path: string, contents: string): void {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, contents);
}

/** A results directory: a run, a tournament, a replay with hostile text, a broken file. */
function results(): string {
  const root = mkdtempSync(join(tmpdir(), "dedale-e2e-results-"));
  put(join(root, "run-a/replay.json"), golden);
  put(join(root, "cup/tournament.json"), readFileSync(join(fixtures, "tournament.json"), "utf8"));
  put(join(root, "cup/games/001/replay.json"), golden);
  put(join(root, "cup/games/002/replay.json"), golden);
  const hostile = JSON.parse(golden);
  hostile.turns[0].players[0].log = [HOSTILE];
  hostile.players[0].about.description = HOSTILE;
  hostile.turns[0].players[0].marks = [
    {
      cells: [
        [0, 0],
        [1, 0],
        [2, 0],
      ],
      label: "route",
      color: "teal",
      kind: "path",
    },
    { cells: [[3, 4]], label: null, color: "red" },
  ];
  hostile.turns[0].players[1].marks = [
    { cells: [[0, 2]], label: "theirs", color: null, kind: "cells" },
  ];
  put(join(root, "hostile/replay.json"), JSON.stringify(hostile));
  put(join(root, "broken/replay.json"), "not json");
  return root;
}

let browser: Browser;

before(async () => {
  browser = await Browser.launch();
});

after(() => {
  browser?.close();
});

/** Clicks the button of `.live-controls` labelled `label`. */
async function press(page: Page, label: string): Promise<void> {
  await page.eval(
    `[...document.querySelectorAll(".live-controls button")].find((b) => b.textContent === ${JSON.stringify(label)}).click()`,
  );
}

test("browsing results: runs, a game, a tournament", async () => {
  const dedale = await Dedale.start(["view", results(), "--no-open"]);
  try {
    const page = await browser.newPage();
    await page.goto(dedale.url);
    // Everything the page loads or does must fit the server's CSP.
    await page.eval(
      "window.violations = []; document.addEventListener('securitypolicyviolation', (e) => window.violations.push(e.violatedDirective))",
    );
    const rows = "document.querySelectorAll('table.runs tbody tr').length";
    assert.equal(await page.waitFor<number>(rows), 4);
    assert.match(await page.text("tr.broken"), /expected/);
    await page.eval(`(() => {
      const filter = document.querySelector("input[type=search]");
      filter.value = "FLAKY";
      filter.dispatchEvent(new Event("input"));
    })()`);
    assert.equal(await page.eval<number>(rows), 3);

    // A game: the maze is drawn, keys step, the inspector shows the log.
    await page.goto(`${dedale.url}#/game/run-a/replay.json`);
    await page.waitFor("document.querySelector('canvas.maze')?.style.width === '200px'");
    const pixel = (x: number, y: number) =>
      page.eval<number[]>(
        `[...document.querySelector("canvas.maze").getContext("2d").getImageData(${x}, ${y}, 1, 1).data]`,
      );
    const floor = await pixel(2, 2);
    assert.notDeepEqual(await pixel(60, 60), floor, "a wall is not the floor");
    assert.notDeepEqual(await pixel(60, 20), floor, "(1, 0) holds a point");
    const head = "document.querySelector('.game-head').textContent";
    assert.match(await page.eval<string>(head), /turn 0 \/ 12/);
    // What the bots say about themselves: only lap says more than its name.
    assert.equal(await page.eval<number>("document.querySelectorAll('ul.about li').length"), 2);
    assert.match(await page.text("ul.about"), /A lap v2 by Ada\s*Laps the maze, clockwise\./);
    await page.press("ArrowRight", "ArrowRight", 39);
    await page.waitFor(`${head}.includes("turn 1 / 12")`);
    assert.match(await page.eval<string>("location.hash"), /\?turn=1$/);
    assert.match(await page.text(".panel.player-0 pre.log"), /on my way/);
    await page.press("End", "End", 35);
    await page.waitFor(`${head}.includes("turn 12 / 12")`);
    assert.deepEqual(await pixel(60, 20), floor, "a collected cell is plain floor");
    assert.match(await page.text(".panel.player-0"), /exit/);
    // The problems strip: a click goes to the turn the thumb would show there.
    const strip = "document.querySelector('canvas.markers')";
    assert.equal(
      await page.eval<boolean>(`${strip}.hidden`),
      false,
      "the golden game has problems",
    );
    await page.eval(`(() => {
      const box = ${strip}.getBoundingClientRect();
      const x = box.left + 8 + (7 / 12) * (box.width - 16);
      ${strip}.dispatchEvent(new MouseEvent("click", { clientX: x, clientY: box.top + 3 }));
    })()`);
    await page.waitFor(`${head}.includes("turn 7 / 12")`);
    await page.press("End", "End", 35);
    await page.waitFor(`${head}.includes("turn 12 / 12")`);
    // Charts (uPlot): three, with the scanner on the turn shown.
    await page.waitFor("document.querySelectorAll('.charts .uplot').length === 3");
    const scanner = "document.querySelector('.charts .scanner')";
    assert.equal(await page.eval<boolean>(`${scanner}.hidden`), false);
    assert.equal(await page.text(".charts .scanner-turn"), "turn 12");
    const atEnd = await page.eval<number>(`parseFloat(${scanner}.style.left)`);
    // The latency legend: the turn shown, then avg / 5% / 1% over the game,
    // as the statistics table has them.
    const legend = (row: number) =>
      page.eval<string[]>(
        `[...document.querySelector(".charts .u-legend").rows[${row}].cells].map((c) => c.textContent)`,
      );
    assert.deepEqual((await legend(0)).slice(1), ["turn 12", "avg", "5%", "1%"]);
    assert.equal(
      (await legend(1)).slice(2).join(" / "),
      await page.eval<string>(`document.querySelector(".stats tbody tr").cells[6].textContent`),
    );
    await page.press("Home", "Home", 36);
    await page.waitFor(`${head}.includes("turn 0 / 12")`);
    assert.equal(await page.eval<boolean>(`${scanner}.hidden`), true, "no scanner before turn 1");
    assert.equal((await legend(0))[1], "turn");
    // A click on a chart shows the turn under it.
    await page.eval(`(() => {
      const over = document.querySelector(".charts .u-over");
      const box = over.getBoundingClientRect();
      const at = { bubbles: true, clientX: box.left + box.width / 2, clientY: box.top + 10 };
      over.dispatchEvent(new MouseEvent("mousedown", at));
      over.dispatchEvent(new MouseEvent("mouseup", at));
    })()`);
    await page.waitFor(`/turn [5-8] \\/ 12/.test(${head})`);
    const middle = await page.eval<number>(`parseFloat(${scanner}.style.left)`);
    assert.ok(middle > 0 && middle < atEnd, `scanner at ${middle}, at the end ${atEnd}`);
    await page.press("End", "End", 35);
    await page.waitFor(`${head}.includes("turn 12 / 12")`);
    await page.choose("Heatmap", "visits:0");
    await page.waitFor("document.querySelector('.legend').textContent.includes('visit')");

    // Bot text stays text.
    await page.goto(`${dedale.url}#/game/hostile/replay.json?turn=1`);
    await page.waitFor(
      `document.querySelector(".panel.player-0 pre.log")?.textContent === ${JSON.stringify(HOSTILE)}`,
    );
    assert.equal(await page.text("ul.about .description"), HOSTILE);
    assert.equal(await page.eval<number>("document.querySelectorAll('main img').length"), 0);
    assert.notEqual(await page.eval<string>("document.title"), "pwned");

    // Marks: a teal line between the first cells' centres, a red cell.
    assert.match(await page.text(".panel.player-0 .marks"), /route: a path through 3 cells/);
    const [r, g, b] = await pixel(35, 20);
    assert.ok(r !== undefined && g !== undefined && b !== undefined);
    assert.ok(r < g && r < b, `teal-ish: ${[r, g, b]}`);
    const [r2, g2, b2] = await pixel(123, 163);
    assert.ok(r2 !== undefined && g2 !== undefined && b2 !== undefined);
    assert.ok(r2 > g2 && r2 > b2, `red-ish: ${[r2, g2, b2]}`);
    assert.notDeepEqual(await pixel(35, 20), floor);
    // B's cell, in B's orange: drawn with both players' marks or B's only.
    const orange = async () => {
      const [r, g, b] = await pixel(3, 83);
      return r !== undefined && g !== undefined && b !== undefined && r > g && g > b;
    };
    const marks = "Marks drawn on the maze";
    assert.equal(await orange(), false, "A's marks by default");
    await page.choose(marks, "both");
    assert.equal(await orange(), true, "both players' marks");
    assert.notDeepEqual(await pixel(35, 20), floor, "A's path with B's marks");
    await page.choose(marks, "1");
    assert.equal(await orange(), true, "B's marks");
    assert.deepEqual(await pixel(35, 20), floor, "no path of A's with B's marks");
    await page.choose(marks, "");
    assert.equal(await orange(), false, "no marks");

    // A tournament, one of its games, and back.
    await page.goto(`${dedale.url}#/tournament/cup/tournament.json`);
    await page.waitFor("document.querySelectorAll('main table').length === 2");
    assert.match(await page.text("ul.about"), /lap v2 by Ada/);
    assert.equal(
      await page.eval<number>("document.querySelectorAll('main table')[0].tBodies[0].rows.length"),
      2,
    );
    await page.eval("document.querySelectorAll('main table')[1].querySelector('a').click()");
    await page.waitFor("location.hash.startsWith('#/game/cup/games/001/replay.json')");
    await page.waitFor(`document.querySelector('.game-head')?.textContent.includes("turn 0 / 12")`);
    await page.eval("history.back()");
    await page.waitFor("location.hash === '#/tournament/cup/tournament.json'");
    await page.waitFor("document.querySelectorAll('main table').length === 2");

    assert.deepEqual(page.errors, []);
    assert.deepEqual(await page.eval<string[]>("window.violations"), []);
  } finally {
    assert.equal(await dedale.stop(), 0);
  }
});

function ring(): { dir: string; map: string } {
  const dir = mkdtempSync(join(tmpdir(), "dedale-e2e-live-"));
  const map = join(dir, "ring.txt");
  writeFileSync(map, RING);
  return { dir, map };
}

test("a live run: started, paced and assembled by the page", async () => {
  const { dir, map } = ring();
  const out = join(dir, "out");
  const bot = binary("dedale-fake-bot");
  const dedale = await Dedale.start([
    "run",
    bot,
    bot,
    "--map",
    map,
    "--max-turns",
    "40",
    "--out",
    out,
    "--view",
    "--no-open",
  ]);
  try {
    const page = await browser.newPage();
    await page.goto(dedale.url);
    await page.waitFor("document.querySelector('.phase')?.textContent === 'waiting'");
    await page.waitFor("document.querySelector('canvas.maze')?.style.width === '200px'");
    assert.deepEqual(dedale.children(), [], "no bot before Start");
    // What the viewer picks on the preview stays once the game is on.
    const chosen = (label: string) =>
      page.eval<string>(`document.querySelector('select[aria-label="${label}"]').value`);
    await page.choose("Marks drawn on the maze", "both");
    await page.choose("Heatmap", "order");
    await page.choose("Speed", "25");

    // The wait between turns sits next to the bots' real time limit.
    assert.equal(await page.text(".live-controls .limit"), "· bots: 100 ms per move");
    await page.choose("Delay between turns", "25");
    await page.waitFor(
      "document.querySelector('.live-controls').textContent.includes('25 ms between turns')",
    );
    await page.click("button.primary");
    await page.waitFor("document.querySelector('.phase').textContent === 'running'");
    await page.waitFor(
      "Number(document.querySelector('.turn-label')?.textContent.split(' ')[1]) >= 3",
    );
    assert.equal(await chosen("Marks drawn on the maze"), "both");
    assert.equal(await chosen("Heatmap"), "order");
    assert.equal(await chosen("Speed"), "25");

    await press(page, "Pause");
    await page.waitFor("document.querySelector('.live-controls').textContent.includes('paused')");
    await sleep(200);
    const held = await page.text(".turn-label");
    await sleep(500);
    assert.equal(await page.text(".turn-label"), held, "no turn while paused");

    await page.choose("Delay between turns", "0");
    await press(page, "Resume");
    await page.waitFor("document.querySelector('.phase').textContent === 'finished'", 20_000);
    const streamed = await page.eval<string>("JSON.stringify(dedaleLive.record())");
    const written = JSON.parse(readFileSync(join(out, "replay.json"), "utf8"));
    assert.deepEqual(JSON.parse(streamed), written);
    assert.equal(written.turns.length, 40);
    const problems =
      written.events.some((e: { kind: string }) => e.kind === "late") ||
      written.turns.some((t: { players: { outcome: string }[] }) =>
        t.players.some((p) => p.outcome !== "ok"),
      );
    assert.equal(
      await page.eval<boolean>("document.querySelector('canvas.markers').hidden"),
      !problems,
      "a strip only with problems",
    );
    assert.deepEqual(page.errors, []);
  } finally {
    assert.equal(await dedale.stop(), 0);
  }
});

test("a live tournament: every game and the leaderboard", async () => {
  const { dir, map } = ring();
  const out = join(dir, "out");
  const bot = binary("dedale-fake-bot");
  const dedale = await Dedale.start([
    "tournament",
    bot,
    bot,
    "--map",
    map,
    "--games",
    "1",
    "--max-turns",
    "10",
    "--out",
    out,
    "--view",
    "--no-open",
  ]);
  try {
    const page = await browser.newPage();
    await page.goto(dedale.url);
    await page.waitFor("document.querySelector('.phase')?.textContent === 'waiting'");
    assert.deepEqual(dedale.children(), []);
    await page.click("button.primary");
    await page.waitFor("document.querySelector('.phase').textContent === 'finished'", 20_000);
    const rows = (i: number) =>
      `document.querySelectorAll(".standings table")[${i}]?.tBodies[0].rows.length`;
    assert.equal(await page.waitFor<number>(`${rows(0)} === 2 && 2`), 2, "leaderboard");
    assert.equal(await page.waitFor<number>(`${rows(1)} === 2 && 2`), 2, "games played");
    for (const [game, dir] of [
      [1, "001-map-duo"],
      [2, "002-map-duo-swapped"],
    ] as const) {
      const streamed = await page.eval<string>(`JSON.stringify(dedaleLive.record(${game}))`);
      const written = JSON.parse(readFileSync(join(out, "games", dir, "replay.json"), "utf8"));
      assert.deepEqual(JSON.parse(streamed), written, `game ${game}`);
    }
    await page.click(".notice a");
    await page.waitFor("location.hash === '#/tournament/tournament.json'");
    await page.waitFor("document.querySelectorAll('main table').length === 2");
    assert.deepEqual(page.errors, []);
  } finally {
    assert.equal(await dedale.stop(), 0);
  }
});
