# dedale (Python)

The API Python bots use to play [Dedale](../../README.md). Dedale embeds this
package and runs bots through it, so nothing needs installing to compete;
`pip install -e bindings/python` only gives editors completion and types.

New to Dedale? [Writing a bot](../../docs/writing-a-bot.md) walks from an
empty file to a competitive bot; this page is the reference.

## A bot

```python
import dedale
from dedale import Move

NAME = "Wall"  # optional: who the bot is
AUTHOR = "Ada"
DESCRIPTION = "Never moves."

def init():
    """Optional: runs once, within the startup budget."""

def move():
    """Runs every turn, within the move budget."""
    return Move.STAY
```

Dedale fills `dedale` with the game before importing the bot, so module-level
code can already read it (that time counts against the startup budget). The
state is updated in place before every `move()`: references to `dedale.me` or
`dedale.points` stay current.

## Reference

### Who the bot is

Three optional globals introduce the bot in leaderboards and the browser
app:

| | |
|---|---|
| `NAME` | What the player is called, instead of its file name |
| `AUTHOR` | Who wrote it |
| `DESCRIPTION` | What it does, in a sentence or two |

Dedale reads them from the bot's file without running it, so they must be
plain string literals assigned at the top level (`NAME = "Wall"`; adjacent
literals in parentheses join as usual). Anything else, such as a computed
value or an assignment inside an `if`, is ignored, with a note in the bot's
stderr log. Dedale keeps them on one line: whitespace and control characters
become single spaces, `NAME` and `AUTHOR` are cut at 64 bytes and
`DESCRIPTION` at 1 KiB.

### Moves

`Move.UP`, `Move.DOWN`, `Move.LEFT`, `Move.RIGHT`, `Move.STAY` (a `str`
enum: `Move.UP == "up"`). `Move.UP.delta` is `(0, -1)`: `y` grows downwards.

`move()` returns a `Move` or a move name in any case (`"up"`, `"Left"`).
Anything else, including `None`, counts as a stay; the replay records what
came back.

### `dedale.maze`

The walls, which never change.

| | |
|---|---|
| `width`, `height` | Size of the grid |
| `rows` | One string per row: `#` wall, `_` open |
| `in_bounds(x, y)` | Inside the grid |
| `is_open(x, y)` | Inside and not a wall |
| `is_wall(x, y)` | A wall, or outside the grid |
| `neighbors(x, y)` | `[(Move, (nx, ny)), ...]`: the moves other than staying that are legal from `(x, y)` |

### Game state

| | |
|---|---|
| `dedale.points` | `set` of `(x, y)` cells that still hold a point |
| `dedale.me` | Your player |
| `dedale.opponent` | The other player, `None` in solo |
| `dedale.players` | Both players, by index |
| `dedale.turn` | The turn being decided; 0 during `init()` |
| `dedale.max_turns` | The turn limit |
| `dedale.move_timeout` | Seconds per `move()` |
| `dedale.startup_timeout` | Seconds for importing the bot and `init()` |
| `dedale.code` | The maze code, `None` for a map file |

A player has `id` (its index), `pos` (`(x, y)`), `score` (points; a point
both players reach together gives each 0.5) and `last_move` (the move applied
last turn, `Move.STAY` when it was rejected, `None` before the first turn).

### Time

`dedale.time_left()` is the number of seconds left in the current call,
counted from when the harness read Dedale's request. Use it to stop a search
in time: a late answer is a stay.

If `move()` overruns, Dedale moves on without it: the turn is a `timeout`
and you stay. The harness then stops `move()` (it raises an exception that
derives from `BaseException`, not `Exception`, so `except Exception` lets it
through), and the next turn starts with its whole budget: a slow move costs
that turn only. Code that must survive being cut off can use `finally`.
The harness uses `SIGALRM` for this (Linux and macOS), so leave that signal
alone. `init()` is never stopped: a slow startup only costs the turns
before it is done.

### Logs and marks

`print()` and `dedale.log(...)` (same arguments as `print`) are captured per
call and stored in the replay, where `dedale replay` shows them turn by turn.

Two calls draw on the maze for the current call, in the browser app and the
terminal views:

- `dedale.mark(cells, label=None, color=None)` highlights cells, such as
  targets or a region.
- `dedale.path(cells, label=None, color=None)` draws a line through cells in
  order, such as the route you plan; consecutive cells should be neighbours.

```python
from dedale import Color

dedale.path(route, label="plan", color=Color.TEAL)
dedale.mark(dangers, label="opponent's", color="red")
```

`color` is one of the eight `Color` members (`RED`, `ORANGE`, `YELLOW`,
`GREEN`, `TEAL`, `BLUE`, `PURPLE`, `PINK`) or their names in any case;
without one, marks take your player's colour, which keeps them apart from
the opponent's when both players' marks are shown. The app picks shades
that read well in light and dark themes. Any other colour raises
`ValueError`.

Per call, Dedale keeps 4 KiB of log, 64 marks (paths included), 4096 marked
cells and labels of up to 64 bytes. Anything beyond is dropped and the log
ends with `[truncated]`. Cells outside the maze are dropped.

### Errors

An exception in `init()` is reported, and the bot still plays: `move()` is
called as usual. An exception in `move()` counts as a stay for that turn. Both
are stored with their traceback. `sys.exit()` ends the bot for the rest of the
game.

Your bot's stdin is empty, and whatever it writes to its stdout, even through
the raw file descriptor, never reaches the protocol. Its stderr goes to a log
file next to the replay.

### In a sandbox

Competitions run bots with `dedale run --sandbox` or `dedale tournament
--sandbox`. A sandboxed bot can read its own directory (sibling modules
still import) and the standard library, write only to `/tmp` (64 MiB), and
has no network and an empty environment; memory and processes are capped.
Only the standard library is guaranteed: packages installed into the
interpreter happen to be visible, but a competition may not have them.
