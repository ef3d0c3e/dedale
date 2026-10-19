# Dedale

A referee for maze bots. One or two competitor programs explore a symmetric
maze from opposite corners and race to collect its points:

- **solo**: clear the maze in as few turns as possible;
- **duo**: collect more points than the opponent.

Bots are separate programs (Python first, any language through a simple
protocol). Each gets a startup budget to precompute, then a time limit per
move. Dedale records every game as a replay with full timing and error
analytics, which it can rewind turn by turn.

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
            dedale.mark(path_to(cell, parent), label="path")
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
| `dedale.mark(cells, label=None, color=None)` | Highlight cells (a planned path, a target) in the replay |
| `Move.UP`, `Move.DOWN`, `Move.LEFT`, `Move.RIGHT`, `Move.STAY` | What `move()` returns (a string like `"up"` works too) |

Anything else `move()` returns, or an exception, counts as a `stay` and is
recorded with its reason or traceback. Your bot's directory is importable, so
helper modules next to it work; Dedale needs nothing installed besides
Python 3.10 or later.

For editor completion, install the package: `pip install -e bindings/python`.

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
`tournament.json` with the leaderboard.

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
Python tests, a stable-toolchain build and `cargo deny`; `--fuzz <seconds>`
also runs each fuzz target.

License: MIT.
