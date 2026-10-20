# Writing a bot

This guide takes you from an empty file to a bot that plays sensibly, shows
you how to watch it think, measure it against the example bots, and get it
ready for a competition. It uses Python; bots in other languages speak the
same game through a [small protocol](protocol.md).

- [What you need](#what-you-need)
- [The game in one minute](#the-game-in-one-minute)
- [A first bot](#a-first-bot)
- [A bot that races](#a-bot-that-races)
- [Watching it play](#watching-it-play)
- [Time](#time)
- [Testing and measuring](#testing-and-measuring)
- [Ideas for a stronger bot](#ideas-for-a-stronger-bot)
- [Before you compete](#before-you-compete)
- [Other languages](#other-languages)

## What you need

- **Python 3.10 or later**, standard library only: nothing to install for
  your bot.
- **Dedale**: the archive for your platform from the
  [releases](https://github.com/ef3d0c3e/dedale/releases) (see
  [Installation](../README.md#installation)), or built from a clone of this
  repository with a [Rust toolchain](https://rustup.rs):

  ```sh
  cargo install --locked --path crates/dedale   # puts `dedale` on your PATH
  ```

  The Python side of Dedale is built into the binary.
- Optionally, completion and types in your editor:
  `pip install -e bindings/python` (the folder is in the archives too).

The commands below run from the repository root or the extracted archive,
where the example bots live in `examples/python/`, with `dedale` on your
PATH (or `./dedale` in the archive).

## The game in one minute

The [rules](../README.md#rules) in full; what matters for a bot:

- The maze is a grid of walls, open cells and points. Coordinates are
  `(x, y)`, from the top-left corner, `y` growing downwards. Every open cell
  can be reached, usually by more than one way.
- Player A starts at `(0, 0)`, player B in the opposite corner. The maze is
  the same seen from both corners (180° rotation), so neither side is
  favoured.
- Every turn, both bots answer at the same time: `up`, `down`, `left`,
  `right` or `stay`. Both moves happen together; players never block each
  other. A move into a wall is illegal and you stay.
- Stepping on a point collects it. If both players step on it in the same
  turn, they split it: half a point each.
- **Solo**, you want to clear the maze in as few turns as you can. **Duo**,
  you want more points than the opponent. The game also ends at a turn
  limit (`4·W·H`).
- Your program gets **5 s** to start (import and `init()`), then **100 ms**
  per move. A late move is lost: you stay that turn.

## A first bot

A bot is a Python file with a `move()` function. Save this as `my_bot.py`:

```python
import dedale
from dedale import Move


def move():
    return Move.STAY
```

and run it:

```sh
dedale run my_bot.py
```

Dedale generates a maze, prints its code (pass it back with `--maze` to play
the same maze again), plays the game and prints the result: this bot stays
put until the turn limit and scores nothing. Before each call to `move()`,
Dedale updates the `dedale` module with the game: `dedale.me.pos` is where
you are, `dedale.points` the cells that still hold a point, `dedale.maze`
the walls. The [Python reference](../bindings/python/README.md) lists it all.

## A bot that races

Walking to the nearest point already beats most first ideas (that is
[`greedy_bot.py`](../examples/python/greedy_bot.py)). In a duo you can do
better by ignoring the points the opponent will reach first. This bot
measures distances through the maze from both players, then heads for the
nearest point it gets to no later than the opponent:

```python
"""Racer: goes for the nearest point it reaches before the opponent."""

from collections import deque

import dedale
from dedale import Move

NAME = "Racer"
AUTHOR = "Your name"
DESCRIPTION = "Goes for the nearest point it reaches before the opponent."
VERSION = 1


def distances(start):
    """Steps from `start` to every open cell, and the first move on the way."""
    steps = {start: 0}
    first = {start: Move.STAY}
    queue = deque([start])
    while queue:
        cell = queue.popleft()
        for move, next_cell in dedale.maze.neighbors(*cell):
            if next_cell not in steps:
                steps[next_cell] = steps[cell] + 1
                first[next_cell] = move if cell == start else first[cell]
                queue.append(next_cell)
    return steps, first


def move():
    if not dedale.points:
        return Move.STAY
    mine, first = distances(dedale.me.pos)
    theirs = distances(dedale.opponent.pos)[0] if dedale.opponent else {}
    # Points I reach no later than the opponent (at the same time, we split).
    ahead = [p for p in dedale.points if mine[p] <= theirs.get(p, mine[p])]
    target = min(ahead or dedale.points, key=lambda p: mine[p])
    dedale.mark([target], label="target")
    return first[target]
```

How it works:

- `dedale.maze.neighbors(x, y)` gives the legal moves from a cell and where
  they lead. A breadth-first search over it gives the number of steps to
  every cell, and the first move of a shortest way there.
- `dedale.opponent` is the other player, or `None` when playing alone; then
  every point counts as one the bot gets first.
- `dedale.mark` highlights the target on the maze in the viewers, which is
  how you will see what the bot is thinking.
- `NAME`, `AUTHOR` and `DESCRIPTION` introduce the bot in result tables and
  the browser app, and `VERSION` says which version plays (raise it when
  you change the bot). They must be plain literals: Dedale reads them
  without running your file.

Play it alone, then against the greedy bot:

```sh
dedale run racer.py
dedale run racer.py examples/python/greedy_bot.py
```

In ten games against greedy (`dedale tournament racer.py
examples/python/greedy_bot.py --games 5 --seed 3`) it won 2, drew 7 and lost
1: a small edge, from one idea.

## Watching it play

- **In the terminal**: `--watch` draws every turn, `--delay 100` slows it
  down, `--marks both` draws both bots' marks (yours in your player's
  colour unless you choose one).

  ```sh
  dedale run racer.py examples/python/greedy_bot.py --watch --delay 100 --marks both
  ```

- **In the browser**: `--view` opens Dedale's app and waits for you to press
  Start. You see the maze with each bot's marks, and for every turn what each
  bot asked for, what happened, how long it took, what it logged and any
  error with its traceback. Charts show the time each move took against the
  budget.

  ```sh
  dedale run racer.py examples/python/greedy_bot.py --view
  ```

- **Afterwards**: every game is saved under `dedale-out/`. `dedale view`
  browses them all in the app; `dedale replay <replay.json>` rewinds one in
  the terminal.

What your bot can leave in the replay:

| | |
|---|---|
| `print(...)` or `dedale.log(...)` | Text attached to that turn |
| `dedale.mark(cells, label, color)` | Highlighted cells: a target, a region |
| `dedale.path(cells, label, color)` | A line through cells, in order: the route you plan |
| An exception | Its traceback, shown on that turn; the bot stays and plays on |
| Anything on stderr | `<file name>.stderr.log` next to the replay |

## Time

**Startup.** Importing your file and running `init()` (optional) must fit in
5 s (`--startup-ms`). That is the time to precompute: distance tables,
routes. A bot that is not ready in time does not lose the game, it stays put
until it is.

**Moves.** Each `move()` must answer within 100 ms (`--move-ms`), measured
by Dedale from sending the turn to receiving the answer, so Python's own
overhead counts. A move that is late is a `timeout`: you stay, the replay
reports it, and Dedale stops your `move()` so that the next turn starts on
time. Keep a margin, and let `dedale.time_left()` (seconds left in the
current call) tell a search when to stop:

```python
import time

import dedale
from dedale import Move


def move():
    # Keep 40% of the budget for answering and Python's own delays.
    deadline = time.monotonic() + dedale.time_left() * 0.6
    best, best_value = Move.STAY, float("-inf")
    for candidate in candidates():  # your own: most promising first
        if time.monotonic() > deadline:
            break
        value = evaluate(candidate)  # your own
        if value > best_value:
            best, best_value = candidate, value
    return best
```

The examples work this way: [`tour_bot.py`](../examples/python/tour_bot.py)
plans with 80% of the startup budget, and
[`rollout_bot.py`](../examples/python/rollout_bot.py) plays games out with
60% of each move's.

The state your module keeps (globals, caches) lasts from one turn to the
next. Each game starts your program afresh: nothing carries over between
games.

## Testing and measuring

- **The same maze again**: `dedale run racer.py --maze <code>` with the code
  a game printed, to check a fix on the case that went wrong.
- **Many games**: `dedale tournament` plays a series and ranks the bots.
  With two bots, each maze is played twice, corners swapped; with one, or
  `--solo`, every bot plays every maze alone and is ranked by turns to clear.
  `--seed` makes the series repeatable.

  ```sh
  dedale tournament racer.py examples/python/rollout_bot.py --games 10 --seed 1
  dedale tournament --solo racer.py examples/python/tour_bot.py --games 10 --seed 1
  ```

- **Problems**: the `failures` column counts every turn that was not `ok`:
  illegal moves, timeouts, errors and the rest. Open the games with `dedale view` and press `n` to jump from one
  problem to the next.
- **Other mazes**: a bot tuned on the defaults (31×31, 40 points) can break
  elsewhere. Try `--size 51x51`, `--size 11x11`, `--points 500`, `--loops 0`
  (dead ends everywhere: a wrong turn costs a long walk back), and tighter
  budgets (`--move-ms 20`).
- **The competition's conditions**: `dedale run --sandbox racer.py` runs it
  the way competitions do (below). Linux, with bubblewrap installed.

## Ideas for a stronger bot

- **Plan a route.** Alone, the best order to visit the points is a
  travelling-salesman problem: `tour_bot` builds one at startup (nearest
  point first, then 2-opt) and clears mazes in about 12% fewer turns than
  greedy.
- **Look ahead.** In a duo, a planned route goes stale as the opponent
  empties it. `rollout_bot` tries each move, plays the game out with both
  players greedy, and keeps the move with the best ending: it beats greedy
  and `tour_bot`.
- **Race and deny.** Points the opponent reaches first are worth less to you;
  points near the opponent's path are worth more, since taking them first
  denies them. Arriving together splits the point.
- **Value clusters.** A point with others close by is worth a detour; an
  isolated one at the end of a corridor may not be.
- **Read the opponent.** `dedale.opponent.pos` and `last_move` show where it
  is heading; many bots are greedy, which makes them predictable.
- **Use the symmetry.** The cell `(x, y)` mirrors
  `(width - 1 - x, height - 1 - y)`: what you precompute for yourself holds
  for the opponent, rotated.

The README's [Beyond greedy](../README.md#beyond-greedy) measures the
examples against each other.

## Before you compete

Competitions run every bot in Dedale's sandbox (`--sandbox`). There, a bot:

- can count on the standard library only (other packages may not be
  there); its own directory is readable, so helper modules next to the bot
  import as usual;
- can write only to a private `/tmp` (64 MiB), has no network and an empty
  environment;
- gets at most 2048 MiB of memory per process, 64 processes and threads, and
  one CPU.

Also:

- Set `NAME`, `AUTHOR` and `DESCRIPTION` as plain strings, and `VERSION`,
  a whole number you raise with every change; `dedale about my_bot.py`
  shows what Dedale reads.
- Leave `SIGALRM` alone: the harness uses it to stop a move that is out of
  time.
- Your stdout is yours: `print` goes to the turn's log, never to Dedale.
- Run a tournament against the examples in the sandbox, on a few maze sizes,
  and check the `failures` column reads 0.

## Other languages

Any executable can compete: Dedale runs bot paths that do not end in `.py`
as programs, and talks to them in JSON lines over stdin and stdout: a
`hello` from the bot, the maze in `init`, `ready` when done, then a `turn`
and a `move` back, until `end`. [`protocol.md`](protocol.md) describes every
message.

More references: the [Python API](../bindings/python/README.md), the
[replay format](replay.md) (to analyse games with your own tools), and the
[README](../README.md).
