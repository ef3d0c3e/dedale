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
    await page.press("ArrowRight", "ArrowRight", 39);
    await page.waitFor(`${head}.includes("turn 1 / 12")`);
    assert.match(await page.eval<string>("location.hash"), /\?turn=1$/);
    assert.match(await page.text(".panel.player-0 pre.log"), /on my way/);
    await page.press("End", "End", 35);
    await page.waitFor(`${head}.includes("turn 12 / 12")`);
    assert.deepEqual(await pixel(60, 20), floor, "a collected cell is plain floor");
    assert.match(await page.text(".panel.player-0"), /exit/);
    assert.equal(await page.eval<number>("document.querySelectorAll('canvas.chart').length"), 3);
    await page.choose("Heatmap", "visits:0");
    await page.waitFor("document.querySelector('.legend').textContent.includes('visit')");

    // Bot text stays text.
    await page.goto(`${dedale.url}#/game/hostile/replay.json?turn=1`);
    await page.waitFor(
      `document.querySelector(".panel.player-0 pre.log")?.textContent === ${JSON.stringify(HOSTILE)}`,
    );
    assert.equal(await page.eval<number>("document.querySelectorAll('main img').length"), 0);
    assert.notEqual(await page.eval<string>("document.title"), "pwned");

    // A tournament, one of its games, and back.
    await page.goto(`${dedale.url}#/tournament/cup/tournament.json`);
    await page.waitFor("document.querySelectorAll('main table').length === 2");
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

    await page.choose("Delay between turns", "25");
    await page.waitFor(
      "document.querySelector('.live-controls').textContent.includes('25 ms between turns')",
    );
    await page.click("button.primary");
    await page.waitFor("document.querySelector('.phase').textContent === 'running'");
    await page.waitFor(
      "Number(document.querySelector('.turn-label')?.textContent.split(' ')[1]) >= 3",
    );

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
