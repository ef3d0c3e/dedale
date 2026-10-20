"""Tour bot: plans its whole route at startup, then follows it.

Greedy walks to the nearest point every turn, so it keeps leaving points
behind and crossing the maze again for them later. This bot spends its
startup budget ordering every point first: nearest neighbour, then 2-opt
(reverse any stretch of the route that makes it shorter), on true maze
distances. Alone in a maze, it clears it in about 15% fewer turns.

It plans for solo games: in a duo the opponent empties parts of the route,
and greedy beats it there. See rollout_bot.py for duos.

Run it with ``dedale run examples/python/tour_bot.py``.
"""

import time
from collections import deque

import dedale
from dedale import Move

NAME = "Tour"
AUTHOR = "Dedale examples"
DESCRIPTION = (
    "Plans its whole route at startup (nearest point first, then 2-opt on "
    "maze distances) and follows it."
)
VERSION = 1

# Moves available from every open cell, filled by init().
neighbors: dict[tuple[int, int], list[tuple[Move, tuple[int, int]]]] = {}
# Every point, in the order the bot visits them (empty: play greedy).
route: list[tuple[int, int]] = []
# The first stop of the route not collected yet.
stop = 0


def init():
    maze = dedale.maze
    for y in range(maze.height):
        for x in range(maze.width):
            if maze.is_open(x, y):
                neighbors[(x, y)] = maze.neighbors(x, y)
    # Keep a margin of the startup budget for safety.
    deadline = time.monotonic() + dedale.time_left() * 0.8
    planned = plan(dedale.me.pos, sorted(dedale.points), deadline)
    if planned is None:
        dedale.log("maze too large to plan in time: playing greedy")
    else:
        route.extend(planned)


def move():
    global stop
    while stop < len(route) and route[stop] not in dedale.points:
        stop += 1
    if stop == len(route):
        # No plan, or the opponent took the rest: the nearest point.
        way = walk(lambda cell: cell in dedale.points)
    else:
        target = route[stop]
        way = walk(lambda cell: cell == target)
        dedale.mark(route[stop + 1 : stop + 30], label="next stops", color="purple")
    if len(way) < 2:
        return Move.STAY
    dedale.path(way, label="route")
    return next(step for step, cell in neighbors[way[0]] if cell == way[1])


def walk(is_target):
    """The cells of a shortest path to the nearest cell is_target accepts,
    from where the bot stands (just that cell when there is none)."""
    start = dedale.me.pos
    parent = {start: None}
    queue = deque([start])
    while queue:
        cell = queue.popleft()
        if is_target(cell) and cell != start:
            way = []
            while cell is not None:
                way.append(cell)
                cell = parent[cell]
            return way[::-1]
        for _, next_cell in neighbors[cell]:
            if next_cell not in parent:
                parent[next_cell] = cell
                queue.append(next_cell)
    return [start]


def distances(source, index):
    """Steps from source to every cell, as a list indexed like `index`."""
    dist = [0] * len(index)
    seen = {source}
    queue = deque([(source, 0)])
    while queue:
        cell, d = queue.popleft()
        dist[index[cell]] = d
        for _, next_cell in neighbors[cell]:
            if next_cell not in seen:
                seen.add(next_cell)
                queue.append((next_cell, d + 1))
    return dist


def plan(start, points, deadline):
    """The points in a short visiting order from start, or None when no
    route can be made before the deadline."""
    if not points:
        return []
    index = {cell: i for i, cell in enumerate(neighbors)}
    stops = [start, *points]
    # dist[i][j]: steps from stop i to stop j (stop 0 is the start).
    dist = []
    for i, source in enumerate(stops):
        if i % 32 == 0 and time.monotonic() > deadline:
            return None
        row = distances(source, index)
        dist.append([row[index[cell]] for cell in stops])

    # Nearest neighbour: always the closest stop not visited yet.
    order, left = [0], set(range(1, len(stops)))
    while left:
        if len(order) % 64 == 0 and time.monotonic() > deadline:
            return None
        here = dist[order[-1]]
        nearest = min(left, key=here.__getitem__)
        order.append(nearest)
        left.remove(nearest)
    first_length = length(order, dist)

    # 2-opt: reversing order[i..j] replaces the steps a->b and c->d by a->c
    # and b->d; keep every reversal that shortens the route. The start
    # stays first and the route does not return, so the last stop has no d.
    n = len(order)
    improved = True
    while improved and time.monotonic() < deadline:
        improved = False
        for i in range(1, n - 1):
            a, b = order[i - 1], order[i]
            for j in range(i + 1, n):
                c = order[j]
                after = dist[c][order[j + 1]] if j + 1 < n else 0
                instead = dist[b][order[j + 1]] if j + 1 < n else 0
                if dist[a][c] + instead < dist[a][b] + after:
                    order[i : j + 1] = reversed(order[i : j + 1])
                    b = order[i]
                    improved = True
            if time.monotonic() > deadline:
                break
    dedale.log(f"route: {n - 1} points in {length(order, dist)} steps (nearest first: {first_length})")
    return [stops[i] for i in order[1:]]


def length(order, dist):
    return sum(dist[a][b] for a, b in zip(order, order[1:]))
