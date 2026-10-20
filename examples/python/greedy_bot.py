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
