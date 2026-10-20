# Dedale

A referee for maze bots. One or two competitor programs explore a symmetric
maze from opposite corners and race to collect its points:

- **solo**: clear the maze in as few turns as possible;
- **duo**: collect more points than the opponent.

Bots are separate programs (Python first, any language through a simple
protocol). Each gets a startup budget to precompute, then a time limit per
move. Dedale records every game as a replay with full timing and error
analytics, which it can rewind turn by turn, in the terminal or in its
browser app, live or afterwards.

```sh
cargo build --release
./target/release/dedale run examples/python/greedy_bot.py examples/python/random_bot.py --watch --delay 50
```

## Rules

- The maze is a grid of walls, empty cells and points, with odd sides
  (default 31×31) and **180° rotational symmetry**: walls and points look the
  same from both corners. Player A starts top-left at `(0, 0)`, player B
  bottom-right. Every open cell is reachable. You choose how many points the
  maze holds (`--points`); by default every open cell except the starts holds
  one.
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
  `--startup-ms` (default 2000). A bot that is late does not stop the game:
  it simply stays put until it is ready.
- **Moves**: each `move()` must answer within `--move-ms` (default 100). A
  late answer counts as `stay`, and the bot is asked for the next turn as
  usual (turns it missed are skipped, so it never falls behind).

## Writing a Python bot

A bot is a Python file with an optional `init()` and a `move()`. The game
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
one idea. Measured on 31×31 mazes with `dedale tournament`:

| Bot | Idea | Solo: turns to clear | Duo against greedy |
|---|---|---|---|
| `tour_bot.py` | Plans the whole route in `init()`: nearest point first, then 2-opt (reverse any stretch that makes the route shorter), on maze distances | 12% fewer than greedy (16% with 100 points) | loses, 4 games to 6 |
| `rollout_bot.py` | For each move, plays the game out with both players greedy, and keeps the move with the best ending | 10% fewer | wins 19 games of 20, one drawn, with 56–58% of the points |

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
dedale tournament examples/python/rollout_bot.py examples/python/greedy_bot.py --games 5 --seed 3
```

Add `--points 100` for sparse mazes, and `--view` to watch them think: the
maze shows `tour_bot`'s route and next stops and `rollout_bot`'s playout,
as each bot draws them.

## Commands

### `dedale run <bot> [<bot>]`

Plays one game. The maze's **code** is printed first: pass it back with
`--maze` to replay the same maze.

| Option | |
|---|---|
| `--maze CODE` | The maze of a code, e.g. `ded2-31x31-l25-p534-9f3a0c11d2e4b6a8` |
| `--size WxH`, `--loops %`, `--points N`, `--seed HEX` | Generate a maze (defaults 31x31, 25, every open cell, random) |
| `--map FILE` | Load a maze from a text file (`#` wall, `.` point, `_` empty) |
| `--startup-ms`, `--move-ms`, `--max-turns` | Timing and turn limit |
| `--swap` | Start the first bot bottom-right |
| `--watch`, `--delay MS` | Draw every turn as it is played |
| `--view` | Follow the game in the [browser app](#browser-app) instead: the bots start when you press Start there, and the page sets the pace. With `--port N`, `--no-open` |
| `--marks a\|b\|none`, `--ascii`, `--color auto\|always\|never` | Drawing options |
| `--out DIR` | Where the replay and logs go (default `dedale-out/<time>-<maze>/`) |
| `--python PATH` | Interpreter for `.py` bots (or `DEDALE_PYTHON`; default `python3`) |

At the end it prints the result and, per bot, the score, steps, startup time,
latency percentiles and any problems (illegal moves, timeouts, errors...).
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
leaderboard after each game.

### `dedale view [DIR]`

Opens the [browser app](#browser-app) over a directory of results (default
`dedale-out/`). `--port N` picks the port; `--no-open` prints the address
without opening a browser. Ctrl-C stops it.

## Browser app

```sh
dedale view                                       # browse dedale-out/
dedale run greedy_bot.py random_bot.py --view     # watch a game live
dedale tournament greedy_bot.py random_bot.py --view
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
  and a strip of markers per player for every turn that went wrong
  (illegal move, unreadable answer, error, timeout, not ready, crash, late
  answer). Click the strip to go there.
- the **inspector**, per player: the move asked for and applied, the
  outcome, latency against the move budget, time inside `move()`,
  position, score, and what the bot logged, marked or raised (with its
  traceback), plus late or ignored answers. Turn 0 shows the startup
  (ready in time or not, import and `init()` time, its log and errors);
  the last turn shows how each bot exited and the end of its stderr. Pick
  whose marks are drawn on the maze: highlighted cells and path lines, in
  the colours the bot chose.
- **heatmaps** under the maze: how often each player stood on each cell, or
  which player took each point, shaded by the order they were taken in.
- **charts**: latency and time inside `move()` per turn (log scale, the move
  budget drawn), and the score of each player over the game. Hover to read
  a turn's values; click to go to it.
- the **statistics** of the game.

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

- **Pause** and **Resume**, and a **delay** between turns, from any tab.
  Dedale holds the game between turns only: a bot's clock starts when it is
  asked to move, so pausing never costs it a timeout. (A bot still
  computing a late answer keeps computing while paused.)
- The view **follows** the newest turn. Go back to study an earlier turn
  and it stays there; **Follow live** catches up again. The inspector,
  heatmaps and charts update as turns arrive.
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
goes to `<name>.stderr.log` next to it. The format is described in
[`docs/replay.md`](docs/replay.md).

## Other languages

Any executable can compete: Dedale runs paths that do not end in `.py` as
programs and talks to them in JSON lines over stdin and stdout. See
[`docs/protocol.md`](docs/protocol.md).

## Development

`scripts/check.sh` runs every gate: rustfmt, clippy (pedantic), the Rust and
Python tests, the browser app's checks and tests (Node 22 or later) and its
end-to-end tests in headless Chromium, a stable-toolchain build and
`cargo deny`; `--fuzz <seconds>` also runs each fuzz
target. The browser app's compiled files (`web/dist/`) are committed, so
building Dedale itself needs only Cargo.

License: MIT.
