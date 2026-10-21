# Dedale

[![CI](https://github.com/ef3d0c3e/dedale/actions/workflows/ci.yml/badge.svg)](https://github.com/ef3d0c3e/dedale/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/ef3d0c3e/dedale)](https://github.com/ef3d0c3e/dedale/releases/latest)

A referee for maze bots. One or two competitor programs explore a symmetric
maze from opposite corners and race to collect its points:

- **solo**: clear the maze in as few turns as possible;
- **duo**: collect more points than the opponent.

Bots are Python programs, each run in its own process. Each gets a startup
budget to precompute, then a time limit per move. Dedale records every game as a replay with full timing and error
analytics, which it can rewind turn by turn, in the terminal or in its
browser app, live or afterwards.

**Compete**: submit your bot to
[dedale-submissions](https://github.com/ef3d0c3e/dedale-submissions) by pull
request. Bots play each other after every merge, and the
[ranking](https://ef3d0c3e.github.io/dedale-submissions/) is published.

[Install it](#installation), then watch two of the example bots race:

```sh
dedale run examples/python/greedy_bot.py examples/python/tour_bot.py --watch --delay 50
```

Want to write a bot? Start with [Writing a bot](docs/writing-a-bot.md), then
[submit it](https://github.com/ef3d0c3e/dedale-submissions#submitting-a-bot).

## Contents

- [Installation](#installation)
- [Documentation](#documentation)
- [Rules](#rules): [timing](#timing)
- [Writing a Python bot](#writing-a-python-bot): [beyond greedy](#beyond-greedy)
- [Commands](#commands): [`run`](#dedale-run-bot-bot), [`maze`](#dedale-maze),
  [`replay`](#dedale-replay-replayjson),
  [`tournament`](#dedale-tournament-bot),
  [running bots you did not write](#running-bots-you-did-not-write),
  [`view`](#dedale-view-dir), [`about`](#dedale-about-bot)
- [Browser app](#browser-app)
- [Analytics](#analytics)
- [Development and tests](#development-and-tests)

## Installation

**Prebuilt.** Each [release](https://github.com/ef3d0c3e/dedale/releases)
has an archive per platform with the `dedale` binary (the browser app and
the Python harness are built in, so bots need only Python 3.10 or later),
the docs, the example bots, and the Python package for editors:

| Platform | Archive |
|---|---|
| Linux x86_64 (static: any distribution) | `dedale-x86_64-unknown-linux-musl.tar.gz` |
| Linux arm64 (static) | `dedale-aarch64-unknown-linux-musl.tar.gz` |
| macOS, Apple silicon | `dedale-aarch64-apple-darwin.tar.gz` |
| macOS, Intel | `dedale-x86_64-apple-darwin.tar.gz` |

```sh
curl -fsSL https://github.com/ef3d0c3e/dedale/releases/latest/download/dedale-x86_64-unknown-linux-musl.tar.gz | tar xz
cd dedale-x86_64-unknown-linux-musl
./dedale run examples/python/greedy_bot.py examples/python/tour_bot.py --watch --delay 50
```

`SHA256SUMS`, next to the archives, lists their checksums. The binaries are
not signed: on macOS, one downloaded with a browser is quarantined, and
`xattr -d com.apple.quarantine dedale` clears it (curl does not quarantine).
On Windows, use the Linux archive in WSL. The sandbox (`--sandbox`) runs on
Linux only.

**From source**, with a [Rust toolchain](https://rustup.rs), 1.95 or
later: in a clone, `cargo install --locked --path crates/dedale` puts
`dedale` on your PATH (or `cargo build --release` builds
`target/release/dedale`). The browser app comes compiled in the
repository, so Node is not needed.

## Documentation

| Page | What it covers |
|---|---|
| [Writing a bot](docs/writing-a-bot.md) | From an empty file to a competitive bot: running it, watching it think, measuring it, getting it ready for a competition |
| [Python API](bindings/python/README.md) | Everything the `dedale` module gives a bot: the maze, the game state, time, logs and marks, errors |
| [Bot protocol](docs/protocol.md) | The JSON lines Dedale and the Python harness exchange: what the replay's logs, marks and outcomes mean |
| [Replay and tournament files](docs/replay.md) | The formats of `replay.json` and `tournament.json`, to analyse games with your own tools |
| [The browser app's server](docs/web.md) | Its routes, security and live events, to script a live session |
| [Working on the browser app](web/README.md) | The app's layout, build, tests and fixtures |

## Rules

- The maze is a grid of walls, empty cells and points, with odd sides
  (default 31×31) and **180° rotational symmetry**: walls and points look the
  same from both corners. Player A starts top-left at `(0, 0)`, player B
  bottom-right. Every open cell is reachable, and corridors loop: by default
  80% of the dead ends are opened (`--loops`), so there is usually more than
  one way to a point. You choose how many points the maze holds
  (`--points`, default 40; more than it has open cells means every open cell
  but the starts).
- Coordinates are `(x, y)`: origin top-left, `x` to the right, `y` down.
- Every turn, each bot answers `up`, `down`, `left`, `right` or `stay`. Both
  moves happen **at the same time**. Characters never block each other: they
  can share or swap cells.
- A move into a wall or off the grid is **illegal**: the character stays and
  the replay records it. So does a late, missing or unreadable answer.
- Stepping on a point collects it. If both characters step on the same point
  in the same turn, they **split it**: half a point each.
- The game ends when every point is collected, when the turn limit is reached
  (default `4·W·H`), or when every bot has crashed.
- Solo, the score is the number of turns needed to clear the maze. Duo, the
  bot with more points wins; equal scores are a draw.

### Timing

- **Startup**: importing the bot and running its `init()` must fit in
  `--startup-ms` (default 5000). A bot that is late does not stop the game:
  it simply stays put until it is ready.
- **Moves**: each `move()` must answer within `--move-ms` (default 100).
  Dedale never waits longer: a turn without an answer is a `timeout`, the
  bot stays, and the problem is reported. A Python `move()` still running
  then is stopped, so the bot takes the next turn with its whole budget and
  a slow move costs that turn only.

## Writing a Python bot

[Writing a bot](docs/writing-a-bot.md) walks through it step by step; in
short, a bot is a Python file with an optional `init()` and a `move()`. The game
state lives in the `dedale` module, which Dedale fills before importing your
file and updates before every call to `move()`. Here is the greedy example
bot from `examples/python/greedy_bot.py`:

```python
"""Greedy bot: walks to the nearest point, breadth-first.

Run it with ``dedale run examples/python/greedy_bot.py``.
"""

from collections import deque

import dedale
from dedale import Move

NAME = "Greedy"
AUTHOR = "Dedale examples"
DESCRIPTION = "Walks to the nearest point, breadth-first."
VERSION = 1

# Moves available from every open cell, filled by init().
neighbors: dict[tuple[int, int], list[tuple[Move, tuple[int, int]]]] = {}


def init():
    """Precomputes the moves from every open cell, within the startup budget."""
    maze = dedale.maze
    for y in range(maze.height):
        for x in range(maze.width):
            if maze.is_open(x, y):
                neighbors[(x, y)] = maze.neighbors(x, y)


def move():
    start = dedale.me.pos
    first = {start: None}  # cell -> first move on the way there
    parent = {start: None}
    queue = deque([start])
    while queue:
        cell = queue.popleft()
        if cell in dedale.points:
            dedale.path(path_to(cell, parent), label="path")
            return first[cell]
        for step, next_cell in neighbors[cell]:
            if next_cell not in first:
                first[next_cell] = first[cell] or step
                parent[next_cell] = cell
                queue.append(next_cell)
    return Move.STAY


def path_to(cell, parent):
    path = []
    while cell is not None:
        path.append(cell)
        cell = parent[cell]
    return path[::-1]
```

`NAME`, `AUTHOR` and `DESCRIPTION` say who the bot is, and `VERSION` which
version of it plays: a whole number from 1, raised whenever the bot
changes. They are optional, and must be plain literals: Dedale reads them
from the file without running it. The player is then called by its `NAME`
rather than its file name (`Greedy v1` in tables), its author shows in the
terminal's tables, and all of them in the replay files and the browser app.

What the `dedale` module offers (full reference in
[`bindings/python/README.md`](bindings/python/README.md)):

| Name | What it is |
|---|---|
| `dedale.maze` | Walls: `width`, `height`, `rows`, `is_wall(x, y)`, `is_open(x, y)`, `in_bounds(x, y)`, `neighbors(x, y)` |
| `dedale.points` | Set of `(x, y)` cells that still hold a point |
| `dedale.me`, `dedale.opponent` | Players: `pos`, `score`, `last_move`, `id` (`opponent` is `None` in solo) |
| `dedale.turn`, `dedale.max_turns` | The turn being decided (0 during `init()`) and the limit |
| `dedale.move_timeout`, `dedale.startup_timeout` | Budgets, in seconds |
| `dedale.time_left()` | Seconds left in the current call |
| `dedale.log(...)` | Like `print()`; both end up in the replay |
| `dedale.mark(cells, label=None, color=None)` | Highlight cells (a target, a region) in the replay and the viewers |
| `dedale.path(cells, label=None, color=None)` | Draw a line through cells, in order (the route you plan) |
| `Color.RED`, …, `Color.PINK` | The 8 colours of marks and paths (names like `"teal"` work too); without one, your player's colour |
| `Move.UP`, `Move.DOWN`, `Move.LEFT`, `Move.RIGHT`, `Move.STAY` | What `move()` returns (a string like `"up"` works too) |

Anything else `move()` returns, or an exception, counts as a `stay` and is
recorded with its reason or traceback. Your bot's directory is importable, so
helper modules next to it work; Dedale needs nothing installed besides
Python 3.10 or later.

For editor completion, install the package: `pip install -e bindings/python`.

### Beyond greedy

Two more examples in `examples/python/` beat the greedy bot, each with
one idea. Measured on the default mazes (31×31, 80% loops, 40 points) with
`dedale tournament`:

| Bot | Idea | Solo: turns to clear | Duo against greedy |
|---|---|---|---|
| `tour_bot.py` | Plans the whole route in `init()`: nearest point first, then 2-opt (reverse any stretch that makes the route shorter), on maze distances | 10% fewer than greedy | loses: no win, 5 draws and 5 losses in 10 games, with 46% of the points |
| `rollout_bot.py` | For each move, plays the game out with both players greedy, and keeps the move with the best ending | 8% fewer | wins 4 games of 10 and draws the other 6, with 51% of the points; and beats `tour_bot` 7 games to none, 3 drawn |

What they show:

- **Alone, it is a routing problem.** Greedy keeps leaving points behind and
  crossing the maze again for them; a route planned with the startup
  budget avoids that.
- **In a duo, it is a race**, decided by wasted steps: both players move
  every turn, so whoever lands on empty cells less often wins. A planned
  route goes stale as the opponent empties it, which is why `tour_bot`
  loses there. Looking ahead does not: `rollout_bot` steps aside from
  races it would lose and takes the points greedy was heading for.
- **Stay within the budgets.** `tour_bot` plans until `dedale.time_left()`
  says to stop (on a maze too large to plan in time it plays greedy), and
  `rollout_bot` stops its playouts the same way.

The tables come from:

```sh
dedale tournament --solo examples/python/greedy_bot.py examples/python/tour_bot.py \
    examples/python/rollout_bot.py --games 6 --seed 1
dedale tournament examples/python/tour_bot.py examples/python/greedy_bot.py --games 5 --seed 3
dedale tournament examples/python/rollout_bot.py examples/python/greedy_bot.py --games 5 --seed 3
dedale tournament examples/python/rollout_bot.py examples/python/tour_bot.py --games 5 --seed 3
```

Add `--loops 25 --points 65535` for the older, fuller mazes, and `--view`
to watch them think: the maze shows `tour_bot`'s route and next stops and
`rollout_bot`'s playout, as each bot draws them.

## Commands

### `dedale run <bot> [<bot>]`

Plays one game. The maze's **code** is printed first: pass it back with
`--maze` to replay the same maze.

| Option | |
|---|---|
| `--maze CODE` | The maze of a code, e.g. `ded2-31x31-l25-p534-9f3a0c11d2e4b6a8` |
| `--size WxH`, `--loops %`, `--points N`, `--seed HEX` | Generate a maze (defaults 31x31, 80, 40, random) |
| `--map FILE` | Load a maze from a text file (`#` wall, `.` point, `_` empty) |
| `--startup-ms`, `--move-ms`, `--max-turns` | Timing and turn limit |
| `--swap` | Start the first bot bottom-right |
| `--watch`, `--delay MS` | Draw every turn as it is played |
| `--view` | Follow the game in the [browser app](#browser-app) instead: the bots start when you press Start there, and the page sets the pace. With `--port N`, `--no-open` |
| `--marks a\|b\|both\|none`, `--ascii`, `--color auto\|always\|never` | Drawing options: whose marks are drawn (default A's), ASCII only, colours |
| `--out DIR` | Where the replay and logs go (default `dedale-out/<time>-<maze>/`) |
| `--python PATH` | Interpreter for `.py` bots (or `DEDALE_PYTHON`; default `python3`) |
| `--sandbox` | Run the bots in a [sandbox](#running-bots-you-did-not-write), with `--memory-mib N` and `--processes N` |

At the end it prints the result and, per bot, the score, steps, startup time,
latency and any problems (illegal moves, timeouts, errors...). Durations read
`avg / 5% / 1%`: the average, then how long the slowest 5% and the slowest 1%
of replies took at least (the 95th and 99th percentiles).
Ctrl-C stops the game and every bot.

A maze code reads `ded2-<W>x<H>-l<loops>-p<points>-<seed>`: generator
version, size, share of dead ends opened into loops, number of points, and
the seed.

Point counts are capped, never rejected. Asking for more points than the maze
has open cells gives every open cell a point; points come in symmetric pairs,
so an odd count needs the centre cell and is rounded down when the maze has
none. The code Dedale prints always carries the number of points actually
placed, so `--points 100000` prints something like `p534`.

### `dedale maze`

Prints a maze in the text format (its code goes to stderr), with the same
maze options as `run`. `dedale maze --size 21x21 > arena.txt` makes a map
file to edit; `--map` checks that it is still symmetric and connected.

### `dedale replay <replay.json>`

Rewinds a game: each view shows the maze after a turn, the moves, outcomes and
latencies, and what the chosen player logged, marked or raised.

| Key | |
|---|---|
| Enter or `n`, `p` | Next and previous turn |
| a number | Jump to that turn |
| `g`, `G` | First and last turn |
| `a`, `b` | Show player A's or B's details and marks |
| `q` | Quit |

`--turn N` prints one view; `--verify` checks the file against the rules
(regenerating the maze and replaying every move).

### `dedale tournament <bot>...`

Plays a series of games and ranks the bots. With two bots it is a duo
tournament: each maze is played twice, corners swapped; a win is worth 2
points, a draw 1. With one bot, or `--solo`, every bot plays every maze alone
and is ranked by mazes cleared, then total turns. `--games N` sets the number
of mazes (default 10); `--seed` is the master seed all maze codes derive from,
so a tournament can be replayed exactly. It writes every game's replay plus a
`tournament.json` with the leaderboard. `--view` follows the whole tournament
in the [browser app](#browser-app): one Start, every game live, the
leaderboard after each game. It takes `run`'s maze, timing and `--sandbox`
options.

### Running bots you did not write

A bot is a program running with your rights: it could read your files or
use the network. `--sandbox` (on `run` and `tournament`, Linux only) runs
each bot with [bubblewrap](https://github.com/containers/bubblewrap)
instead. A sandboxed bot:

- sees the system read-only (`/usr`, the libraries), its own directory and
  the Python installation that runs it read-only (with its packages, or
  those of the virtual environment it comes from), and nothing else of your
  files: packages from `pip install --user` stay hidden in your home;
- writes only to a private `/tmp` of 64 MiB, which disappears with it;
- has no network and an empty environment (no tokens or secrets from
  yours), and cannot see other processes;
- gets at most 2048 MiB of memory per process (`--memory-mib`) and 64
  processes and threads (`--processes`);
- runs on a CPU of its own, on separate physical cores when there are
  enough; Dedale keeps off those CPUs.

It needs `bwrap`, `prlimit` and `taskset` (util-linux). Dedale checks that
they work before the first game and stops with an explanation if they do
not. On Ubuntu 24.04 and later, AppArmor blocks the unprivileged user
namespaces bubblewrap uses: load bwrap's AppArmor profile, or run
`sudo sysctl -w kernel.apparmor_restrict_unprivileged_userns=0`. The replay
notes that the game was sandboxed and which CPU each bot had.

A bot can still change its own CPU affinity: pinning keeps honest bots
apart, it does not stop a determined one.

### `dedale view [DIR]`

Opens the [browser app](#browser-app) over a directory of results (default
`dedale-out/`). `--port N` picks the port; `--no-open` prints the address
without opening a browser. Ctrl-C stops it.

### `dedale about <bot>...`

Prints what each bot says about itself, one JSON object per line, without
playing: `path`, `said_hello` (false when the bot did not start or said no
`hello` in time), `name`, `author`, `description` and `version` (`null`
when not given). They are read from the bots' source, so none of their
code runs.

```sh
$ dedale about examples/python/greedy_bot.py
{"path":"examples/python/greedy_bot.py","said_hello":true,"name":"Greedy","author":"Dedale examples","description":"Walks to the nearest point, breadth-first.","version":1}
```

It exits with 0 whatever the bots say, leaving the judging to the caller (a
competition's submission checks, for instance). `--hello-ms` sets how long
a bot may take to say `hello` (default 10000); `--python` and `--sandbox`
as for `run`.

## Browser app

```sh
dedale view                                       # browse dedale-out/
dedale run greedy_bot.py tour_bot.py --view       # watch a game live
dedale tournament greedy_bot.py tour_bot.py --view
```

Dedale serves the app itself and opens it in your browser (`xdg-open`, or
`open` on macOS; `--no-open` only prints the address, `--port N` fixes the
port). It listens on `127.0.0.1` only, and the printed address carries a
secret token: other pages and users cannot reach your results or steer a
game. Nothing to install: the app is built into the binary.

**Runs.** The home page lists every game and tournament under the directory,
newest first, with bots, result and maze. Type in the filter to keep the
ones a bot played. A file Dedale cannot read is listed with the reason.

**A game.** Choose a turn and see everything about it:

- the **maze**: walls, the points left, and the players (A blue, B orange;
  a split disc when they share a cell). Hover a cell for its position and
  the turn its point was taken.
- the **header**: turn and turn limit, points left, scores, maze code, and
  the result.
- the **timeline**: a scrubber, step and play buttons, the playback speed,
  and, under the scrubber, a strip of markers per player for every turn
  that went wrong (illegal move, unreadable answer, error, timeout, not
  ready, crash, late answer), each under the scrubber's place for its turn;
  no strip when nothing went wrong. Click the strip to go there.
- the **inspector**, per player: the move asked for and applied, the
  outcome, latency against the move budget, time inside `move()`,
  position, score, and what the bot logged, marked or raised (with its
  traceback), plus late or ignored answers. Turn 0 shows the startup
  (ready in time or not, import and `init()` time, its log and errors);
  the last turn shows how each bot exited and the end of its stderr. Pick
  whose marks are drawn on the maze (A's, B's, both players' or none):
  highlighted cells and path lines, in the colours the bot chose or else its
  player's. With both players' marks, labels start with the player's letter
  (`A route`, `B path`).
- **heatmaps** under the maze: how often each player stood on each cell, or
  which player took each point, shaded by the order they were taken in.
- **charts**: latency and time inside `move()` per turn (log scale, the move
  budget dashed, problems as red dots), and each player's score with the
  points left. A scanner marks the turn shown on every chart and follows it
  as you step, play or seek; the legends read that turn's values, or the
  turn under the pointer, and for durations their avg / 5% / 1% over the
  turns in view. Hover any chart to see every chart's cursor, drag across
  one to zoom them all on those turns (and their avg / 5% / 1% on them),
  double-click to see the whole game again, and click to go to a turn.
- the **statistics** of the game: latency and time in `move()` as
  avg / 5% / 1%.

| Key | |
|---|---|
| ← → | Previous and next turn |
| space | Play or pause |
| Home, End | First and last turn |
| `p`, `n` | Previous and next problem |

The address follows the turn shown (`#/game/<file>?turn=N`): bookmark or
share it.

**A tournament.** The leaderboard and every game with its result; click a
game to open it, and Back to return.

**Live** (`--view`). Dedale prints the address and waits: nothing runs until
you press **Start** in the page (it shows the bots, the timings and, for a
single game, the maze). Then:

- **Pause** and **Resume**, and a **wait between turns**, from any tab.
  Both are for watching: Dedale holds the game between turns only, and a
  bot's clock starts when it is asked to move, so they never cost a timeout
  and never change the bots' time per move (`--move-ms`, shown next to
  them).
- The view **follows** the newest turn. Go back to study an earlier turn
  and it stays there; **Follow live** catches up again. The inspector,
  heatmaps and charts update as turns arrive.
- What you choose to see (whose marks, the heatmap, the charts, the
  playback speed) stays from the preview to the game, and from one game of
  a tournament to the next.
- A tournament shows the leaderboard after each game, the games played (each
  opens as a replay) and the game being played.
- A page that loses its connection reconnects by itself and catches up;
  several tabs can watch at once.

The output, replays and logs are the same as without `--view`. After the
end Dedale keeps serving the page, so the finished games stay browsable,
until Ctrl-C.

To script against a live session (start it, pace it, follow its events), see
[`docs/web.md`](docs/web.md).

## Analytics

Each game's `replay.json` holds everything: the maze, the configuration,
every turn (requested and applied moves, outcomes, positions, points,
latency measured by Dedale, time spent inside `move()`, logs, marks, errors),
events such as late answers, the result, and per-bot statistics (latency
min/mean/p50/p95/p99/max, outcome counts, steps, startup time). Bots' stderr
goes to `<file name>.stderr.log` next to it. The format is described in
[`docs/replay.md`](docs/replay.md).

## Development and tests

`scripts/check.sh` runs every gate: rustfmt, clippy (pedantic), the Rust and
Python tests, the browser app's checks and tests (Node 22 or later) and its
end-to-end tests in headless Chromium, a stable-toolchain build and
`cargo deny`; `--fuzz <seconds>` also runs each fuzz
target. The browser app's compiled files (`web/dist/`) are committed, so
building Dedale itself needs only Cargo.

The suites one by one:

| Suite | Command | What it covers |
|---|---|---|
| Rust | `cargo test --workspace --all-features` | Unit and property tests (generator, rules, protocol, replay, stats), and integration tests that play real games with Python bots and a scripted fake bot: timeouts, crashes, the sandbox, the CLI, the web server |
| Python | `cd bindings/python && python3 -m unittest discover -s tests -t .` | The harness driven through its pipes, and the `dedale` API |
| Browser app | `cd web && npm ci && npm run check && npm test` | Type checks, lint, and the app's data layer on fixtures written by the Rust tests |
| End to end | `cd web && npm run e2e` (after the Rust tests, whose debug build it drives) | The `dedale` binary and the app in headless Chromium (`DEDALE_CHROMIUM` picks the browser): browsing results, replays, live games |
| Fuzzing | `scripts/check.sh --fuzz 60` (seconds per target) | The parsers and decoders (bot lines, maze codes, map files, replays, HTTP requests), the generator and the game engine, with cargo-fuzz on nightly |

GitHub Actions runs the same script on every push to `master` and every
pull request ([`.github/workflows/ci.yml`](.github/workflows/ci.yml)), on
Ubuntu 24.04 with a pinned Rust, the runner's Chrome, and bubblewrap so
that the sandbox tests run (`DEDALE_REQUIRE_SANDBOX=1` makes them fail
rather than skip when the sandbox cannot start). A macOS job builds
`dedale` and plays a game with it (the tests themselves need Linux).
Dependabot proposes updates of the crates, the app's tools and the pinned
actions every week ([`.github/dependabot.yml`](.github/dependabot.yml)).

**Releases.** Set the new version in `Cargo.toml` and
`bindings/python/pyproject.toml`, describe it in
[`CHANGELOG.md`](CHANGELOG.md), commit, then tag and push the tag
(`git tag v0.2.0 && git push origin v0.2.0`). The
[release workflow](.github/workflows/release.yml) checks that the tag, both
versions and the changelog agree, runs the CI checks, builds `dedale` for each platform
(`--profile dist`: thin LTO, stripped), has each binary play and verify a
game, and drafts a GitHub release with the archives and `SHA256SUMS`, to
review and publish. Run by hand (Actions, Release, Run workflow), it builds
the archives as workflow artifacts and releases nothing.

License: MIT.
