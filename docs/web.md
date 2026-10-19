# The browser app's server

`dedale view`, `dedale run --view` and `dedale tournament --view` serve the
browser app from Dedale itself. This page describes that server: its
security, its HTTP API, and the live events, for anyone scripting against a
session. The app is the only client Dedale ships; nothing here is needed to
use it.

## Security

- The server listens on `127.0.0.1` only (`--port N`, or any free port).
- Every request must name the server in `Host`: `127.0.0.1:<port>` or
  `localhost:<port>`, so a web page on another site cannot reach it through
  DNS rebinding (`403` otherwise, `400` without one).
- `/api/` requests need the session's token, 128 random bits printed in the
  address (`?token=<hex>`, compared in constant time; `403` otherwise). The
  app's own files (`/`, `/assets/…`) do not: they are the same static files
  as in the binary, and the page reads the token from its own address.
- Only the embedded app and `replay.json` and `tournament.json` files under
  the served directory are ever served: other names, `..`, absolute paths
  and symbolic links out of the directory (or to other files) are refused.
- Responses forbid framing, inline scripts and styles and remote resources
  (`Content-Security-Policy`), send no referrer, and are never cached.
- Requests are HTTP/1.1, one per connection, at most 8 KiB of request line
  and headers and 4 KiB of body; at most 64 connections at once (`503`
  beyond).

The token is in the address Dedale passes to `xdg-open` (or `open`), whose
command line other users of the machine can see while it runs. On a shared
machine, use `--no-open` and open the printed address yourself.

## Routes

`<token>` stands for the token of the printed address. Errors are JSON:
`{"error": "…"}`.

| Route | Modes | |
|---|---|---|
| `GET /` | all | The app |
| `GET /assets/<file>` | all | The app's files |
| `GET /api/session?token=` | all | What this server is (below) |
| `GET /api/runs?token=` | all | Games and tournaments under the directory (below) |
| `GET /api/file?path=<relative>&token=` | all | A `replay.json` or `tournament.json` ([formats](replay.md)) |
| `GET /api/events?token=` | live | The live events, as [server-sent events](https://html.spec.whatwg.org/multipage/server-sent-events.html) |
| `POST /api/start?token=` | live | Start the run or tournament (`409` once started) |
| `POST /api/control?token=` | live | Change the pace: `{"paused": true}`, `{"delay_ms": 250}`, or both; answers the new pace |

In live modes the served directory is the output directory (`--out`), so
`/api/file` and `/api/runs` show the games written so far.

For example, with the address `http://127.0.0.1:41234/?token=ab12…`:

```sh
curl -N 'http://127.0.0.1:41234/api/events?token=ab12…'
curl -X POST 'http://127.0.0.1:41234/api/control?token=ab12…' -d '{"delay_ms": 100}'
curl -X POST 'http://127.0.0.1:41234/api/start?token=ab12…'
```

### `/api/session`

Browsing: `{"mode": "browse", "title": "<directory>"}`.

Live: `mode` (`run` or `tournament`), `title`, `bots`, `games` (planned),
`settings` (mazes and timings, in a line), `tournament` (`duo`, `solo`, or
`null` for a run), `start` (for a run, the game it will play, as in
`game_start` below; `null` for a tournament), `phase` (`waiting`, `running`,
`finished`, `failed`), `error` (why it failed), `file` (once finished,
`replay.json` or `tournament.json`).

### `/api/runs`

`{"entries": [...], "truncated": false}`, newest first. A game:
`{"kind": "game", "path", "modified", "error", "code", "seating", "bots",
"result"}` (`path` of its `replay.json`, `modified` in seconds since 1970).
A tournament: `{"kind": "tournament", "path", "modified", "error", "mode",
"bots", "leaders", "games"}`, its `games` listed as game entries (without
`kind`) rather than on their own. A file that cannot be read keeps its path
and gives the reason in `error`. Hidden directories and symbolic links are
skipped; the search stops after 8 levels and 10 000 directories
(`truncated`).

## Live events

Each event is a name and one line of JSON, built from the
[replay types](replay.md):

```text
event: turn
data: {"game":1,"turn":{"turn":7,"players":[…],"collected":[[3,0]]}}
```

| Event | Data |
|---|---|
| `snapshot` | Always first: `session` (as `/api/session`), `pace`, `games` (each: `game`, `start`, `startups`, `turns`, `end`), `standings` |
| `waiting` | `{}`, after the snapshot while nobody has pressed Start |
| `game_start` | `game` (from 1), `code`, `maze`, `config`, `players` (`name`, `path`, `start`) |
| `startup` | `game`, `player`, `startup` (its startup record) |
| `turn` | `game`, `turn` (its turn record) |
| `game_end` | `game`, `format`, `dedale`, `players` (full player records), `events`, `result`, `stats`, `replay` (its path, for `/api/file`) |
| `standings` | `standings`: the tournament's leaderboard after each game |
| `pace` | `paused`, `delay_ms`: after any change |
| `finished` | `file`: `replay.json` or `tournament.json` |
| `error` | `message`: the session failed |

A game's record is its `game_start` (`code`, `maze`, `config`), its `turn`s
in order, and its `game_end`: the same object as the `replay.json` Dedale
writes. `game_end` comes once that file is written.

A `startup` comes when a bot becomes ready (`ok` or `late`), its `init()`
fails, it crashes before being ready, or the startup budget runs out first
(`timeout`, followed by `late` if it gets ready afterwards); the last one is
the recorded one.

In a snapshot, a game still being played has its turns so far; a finished
game has none (they are in its replay) but has its `end`. A connection that
falls more than 4096 events behind is closed; the browser reconnects
(`retry: 1000`) and gets a fresh snapshot, so nothing is lost. A comment
line is sent after 15 s of silence.

The app keeps the game it assembled in the console:
`dedaleLive.record()` (the latest game) or `dedaleLive.record(n)`.

## Pacing

The pace holds the game **between** turns: before sending a turn, Dedale
waits while paused, or until `delay_ms` after the previous turn. A bot's
clock starts when it is sent the turn, so pacing never makes it late.
Ctrl-C still stops a held game within 100 ms.
