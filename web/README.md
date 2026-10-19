# Dedale's browser app

The app behind `dedale view` and `--view` (user guide: the
[README](../README.md#browser-app); server and events:
[`docs/web.md`](../docs/web.md)). TypeScript, no framework and no runtime
dependency: the maze and charts are drawn on canvas by hand.

## Layout

| Path | |
|---|---|
| `src/types.ts` | The data Dedale writes and streams, as the Rust types serialize it |
| `src/validate.ts` | Checks loaded JSON against those types, with errors naming the field |
| `src/api.ts`, `src/url.ts` | The server's API, the token, the event stream (reconnecting) |
| `src/router.ts` | Hash routes: `#/`, `#/game/<path>?turn=N`, `#/tournament/<path>`, `#/live` |
| `src/model.ts` | A game's state at any turn, from the stored positions and collections |
| `src/live.ts` | A live session as events left it, and the records it assembles |
| `src/live/controller.ts` | What the live page shows: following, pace, which game |
| `src/game/` | The game screen: canvas, timeline, inspector, charts, heatmaps (`screen.ts` puts them together) |
| `src/results.ts`, `src/table.ts` | The run browser's and tournament view's lists |
| `src/views/` | One view per route; `src/app.ts` is the shell |
| `src/dom.ts`, `src/format.ts` | Building elements; numbers and words as the terminal prints them |
| `static/` | `index.html` and the stylesheet, copied as they are |
| `test/` | `node --test` unit tests; `test/fixtures/` comes from Rust (below) |
| `e2e/` | End-to-end tests: the `dedale` binary and headless Chromium |
| `dist/` | The built app, **committed**, embedded in the binary |

Modules import each other with `.ts` extensions, which `tsc` rewrites to
`.js`: the browser loads plain ES modules, with no bundler.

## Working on it

Node 22 or later, then in `web/`:

```sh
npm ci            # the pinned tools: typescript, @biomejs/biome, @types/node
npm run check     # tsc strict (app, tests, scripts) and Biome (format, lint)
npm run fix       # Biome's fixes and formatting
npm test          # unit tests, compiled by tsc into build/ and run by node --test
npm run build     # dist/: commit it with your change
npm run e2e       # end to end; needs ../target/debug/dedale (cargo build --all-features)
```

`crates/dedale/build.rs` embeds every file of `dist/` in the binary, so
rebuild Dedale (`cargo build`) to see a change through `dedale view`.
`scripts/check.sh` runs all of the above and fails when `dist/` is not what
the sources build.

The end-to-end tests launch the installed Chromium (`DEDALE_CHROMIUM` to
choose another) and drive it over the DevTools protocol with Node's own
WebSocket (`e2e/cdp.ts`); `DEDALE_BIN` runs another `dedale`.

## Fixtures

`test/fixtures/` holds the golden replay of the Rust tests, the state after
each of its turns as Rust rebuilds it, a tournament file and the event
stream a live session sends for that game. They are written by a Rust test,
so both sides read the same data and must rebuild the same states:

```sh
DEDALE_BLESS=1 cargo test --all-features --test web_fixtures
```

Without `DEDALE_BLESS` the test fails when a fixture is out of date.

## Rules of the house

- Text from bots (logs, labels, errors) is only ever shown through text
  nodes (`dom.ts`), never as HTML.
- The page's policy forbids inline scripts and `style` attributes: set
  styles through `element.style`, and keep colours bots choose to plain
  colour values (`markColour`).
- Pure logic lives apart from the DOM, so `node --test` covers it; the DOM
  and canvas are covered end to end.
