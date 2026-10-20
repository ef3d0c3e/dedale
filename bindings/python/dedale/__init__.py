"""Python API for Dedale bots.

A bot is a module with an optional ``init()`` and a ``move()``. Dedale's
harness fills this module with the game state before importing the bot, and
updates it in place before every call to ``move()``::

    import dedale
    from dedale import Move

    def init():
        ...  # precompute, within the startup budget

    def move():
        x, y = dedale.me.pos
        return Move.UP  # or "up"

Coordinates are ``(x, y)``: origin top-left, ``x`` to the right, ``y`` down.
Anything printed during ``init()`` or ``move()`` is captured and stored in the
replay, as are cells highlighted with :func:`mark` and routes drawn with
:func:`path`.
"""

from __future__ import annotations

import enum
import sys
import time
from collections.abc import Iterable

__all__ = [
    "Color",
    "Maze",
    "Move",
    "Player",
    "code",
    "log",
    "mark",
    "max_turns",
    "maze",
    "me",
    "move_timeout",
    "opponent",
    "path",
    "players",
    "points",
    "startup_timeout",
    "time_left",
    "turn",
]


class Move(str, enum.Enum):
    """What a character does in one turn."""

    UP = "up"
    DOWN = "down"
    LEFT = "left"
    RIGHT = "right"
    STAY = "stay"

    @property
    def delta(self) -> tuple[int, int]:
        """``(dx, dy)``, with ``y`` growing downwards."""
        return _DELTAS[self]

    def __str__(self) -> str:
        return self.value


_DELTAS = {
    Move.UP: (0, -1),
    Move.DOWN: (0, 1),
    Move.LEFT: (-1, 0),
    Move.RIGHT: (1, 0),
    Move.STAY: (0, 0),
}
_STEPS = [(m, _DELTAS[m]) for m in (Move.UP, Move.DOWN, Move.LEFT, Move.RIGHT)]


class Color(str, enum.Enum):
    """The colours of :func:`mark` and :func:`path`; the viewers choose the shades."""

    RED = "red"
    ORANGE = "orange"
    YELLOW = "yellow"
    GREEN = "green"
    TEAL = "teal"
    BLUE = "blue"
    PURPLE = "purple"
    PINK = "pink"

    def __str__(self) -> str:
        return self.value


class Maze:
    """The walls of the maze. They never change; points live in :data:`points`."""

    __slots__ = ("width", "height", "rows", "_open")

    def __init__(self, width: int, height: int, cells: str) -> None:
        self.width = width
        self.height = height
        self._open = [c != "#" for c in cells]
        #: One string per row: ``#`` for a wall, ``_`` for an open cell.
        self.rows = tuple(
            "".join("#" if c == "#" else "_" for c in cells[y * width : (y + 1) * width])
            for y in range(height)
        )

    def in_bounds(self, x: int, y: int) -> bool:
        return 0 <= x < self.width and 0 <= y < self.height

    def is_wall(self, x: int, y: int) -> bool:
        """True for walls, and for anything outside the grid."""
        return not self.is_open(x, y)

    def is_open(self, x: int, y: int) -> bool:
        return 0 <= x < self.width and 0 <= y < self.height and self._open[y * self.width + x]

    def neighbors(self, x: int, y: int) -> list[tuple[Move, tuple[int, int]]]:
        """The legal moves from ``(x, y)`` other than staying, and where they lead."""
        return [(m, (x + dx, y + dy)) for m, (dx, dy) in _STEPS if self.is_open(x + dx, y + dy)]

    def __repr__(self) -> str:
        return f"Maze({self.width}x{self.height})"


class Player:
    """A character. Updated in place every turn, so references stay current."""

    __slots__ = ("id", "pos", "score", "last_move")

    def __init__(self, id: int, pos: tuple[int, int]) -> None:
        self.id = id
        #: ``(x, y)`` after the last turn.
        self.pos = pos
        #: Points collected; a point both players reach together is worth 0.5 each.
        self.score = 0.0
        #: The move applied last turn (``STAY`` when rejected), ``None`` before turn 1.
        self.last_move: Move | None = None

    def __repr__(self) -> str:
        return f"Player(id={self.id}, pos={self.pos}, score={self.score})"


# Game state, filled by the harness. The annotations are for editors; the
# values exist from the moment the bot module is imported.
maze: Maze = None  # type: ignore[assignment]
#: Cells that still hold a point.
points: set[tuple[int, int]] = set()
me: Player = None  # type: ignore[assignment]
#: The other player, or ``None`` in a solo game.
opponent: Player | None = None
players: list[Player] = []
#: The turn being decided (0 during ``init()``).
turn: int = 0
max_turns: int = 0
#: Seconds per ``move()`` call.
move_timeout: float = 0.0
#: Seconds for importing the bot and running ``init()``.
startup_timeout: float = 0.0
#: The maze code (to replay this maze), or ``None`` for a maze from a file.
code: str | None = None

# Set by the harness for each call.
_deadline = float("inf")

# Marks of the current call, capped like everything a bot sends.
MAX_MARKS = 64
MAX_MARK_CELLS = 4096
MAX_LABEL = 64
_marks: list[dict] = []
_mark_cells = 0
_marks_truncated = False


def time_left() -> float:
    """Seconds left in the current call, measured from when the harness read the request."""
    return max(0.0, _deadline - time.perf_counter())


def log(*args: object, sep: str = " ") -> None:
    """Like ``print()``: the text is attached to this call in the replay."""
    print(*args, sep=sep, file=sys.stdout)


def mark(
    cells: Iterable[tuple[int, int]], label: str | None = None, color: Color | str | None = None
) -> None:
    """Highlights ``cells`` for this call in the replay and the debugger.

    ``color`` is a :class:`Color` (or its name, in any case); without one the
    cells take the player's colour. Cells outside the maze are dropped. Past
    64 marks or 4096 cells per call (paths included), the rest is dropped and
    the log says ``[truncated]``.

    Raises :class:`ValueError` for a colour that is not one of :class:`Color`.
    """
    _add(cells, label, color, "cells")


def path(
    cells: Iterable[tuple[int, int]], label: str | None = None, color: Color | str | None = None
) -> None:
    """Draws a line through ``cells``, in order, for this call in the replay
    and the debugger: a planned route. Consecutive cells should be neighbours.

    Same colours and limits as :func:`mark`.
    """
    _add(cells, label, color, "path")


def _color(color: Color | str | None) -> str | None:
    if color is None:
        return None
    try:
        return Color(str(color).lower()).value
    except ValueError:
        names = ", ".join(c.value for c in Color)
        raise ValueError(f"color must be one of {names} (or None), not {color!r}") from None


def _add(cells: Iterable[tuple[int, int]], label: object, color: Color | str | None, kind: str) -> None:
    global _mark_cells, _marks_truncated
    color_name = _color(color)
    if len(_marks) >= MAX_MARKS or _mark_cells >= MAX_MARK_CELLS:
        _marks_truncated = True
        return
    kept = []
    for x, y in cells:
        x, y = int(x), int(y)
        if maze is not None and not maze.in_bounds(x, y):
            continue
        if _mark_cells + len(kept) >= MAX_MARK_CELLS:
            _marks_truncated = True
            break
        kept.append([x, y])
    _mark_cells += len(kept)
    _marks.append({"cells": kept, "label": _cap(label), "color": color_name, "kind": kind})


def _cap(text: object) -> str | None:
    if text is None:
        return None
    data = str(text).encode()
    return data[:MAX_LABEL].decode(errors="ignore")


def _take_marks() -> tuple[list[dict], bool]:
    """The current call's marks, and whether some were dropped; resets them."""
    global _marks, _mark_cells, _marks_truncated
    taken = (_marks, _marks_truncated)
    _marks, _mark_cells, _marks_truncated = [], 0, False
    return taken
