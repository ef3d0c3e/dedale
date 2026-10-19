# dedale (Python)

The API Python bots use to play [Dedale](../../README.md). Dedale embeds this
package and runs bots through it, so nothing needs installing to compete;
`pip install -e bindings/python` only gives editors completion and types.

## A bot

```python
import dedale
from dedale import Move

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

If `move()` overruns, Dedale moves on without it. When it finally returns,
the harness skips the turns it missed and answers the newest one, so a slow
turn costs turns, never synchronization.

### Logs and marks

`print()` and `dedale.log(...)` (same arguments as `print`) are captured per
call and stored in the replay, where `dedale replay` shows them turn by turn.

`dedale.mark(cells, label=None, color=None)` highlights cells for this call,
such as a planned path or a target. The replay viewer draws them over the maze.

Per call, Dedale keeps 4 KiB of log, 64 marks and 4096 marked cells, and
labels and colours of up to 64 bytes. Anything beyond is dropped and the log
ends with `[truncated]`. Marked cells outside the maze are dropped.

### Errors

An exception in `init()` is reported, and the bot still plays: `move()` is
called as usual. An exception in `move()` counts as a stay for that turn. Both
are stored with their traceback. `sys.exit()` ends the bot for the rest of the
game.

Your bot's stdin is empty, and whatever it writes to its stdout, even through
the raw file descriptor, never reaches the protocol. Its stderr goes to a log
file next to the replay.
