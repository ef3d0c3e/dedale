"""Rollout bot: tries every move, plays the game out, keeps the best.

Greedy never thinks about its opponent. This bot does, by simulation: for
each legal move it plays the rest of the game forward with both players
walking greedily (each to its nearest point), and keeps the move with the
best ending: the biggest lead in a duo, the fewest turns alone. It mostly
plays greedy's own move, and steps aside when that loses a race or leaves
greedy a point it was about to take, so greedy walks over empty cells.

Its model of the opponent is greedy itself, so against greedy its playouts
are exact, and it only leaves greedy's move when one shows a better
ending: it wins nearly every duo game against greedy. Alone, it clears
mazes in about 10% fewer turns.

Run it with ``dedale run examples/python/rollout_bot.py examples/python/greedy_bot.py``.
"""

import time
from collections import deque

import dedale
from dedale import Move

NAME = "Rollout"
AUTHOR = "Dedale examples"
DESCRIPTION = (
    "Tries every move, plays the game out with both players greedy, and "
    "keeps the move with the best ending."
)
VERSION = 1

# In a duo, the turns a playout looks ahead: enough to see races through,
# short enough to finish in time on large mazes. Solo playouts go to the
# end: the turns needed to clear the maze are what counts.
HORIZON = 300
# Turns played greedily after a playout ran out of time, before trying
# again (as points go, playouts get shorter).
REST = 50

# The maze as a graph of cell numbers (y * width + x), filled by init().
width = 0
adjacent: list[list[int]] = []
step_to: dict[tuple[int, int], Move] = {}

# Cells this bot stood on lately, and turns left to play greedily: two
# rollout bots can otherwise both step away from the same point forever.
recent: deque[int] = deque(maxlen=6)
calm = 0
rest = 0


def init():
    global width
    maze = dedale.maze
    width = maze.width
    adjacent.extend([] for _ in range(maze.width * maze.height))
    for y in range(maze.height):
        for x in range(maze.width):
            if maze.is_open(x, y):
                for step, (nx, ny) in maze.neighbors(x, y):
                    adjacent[y * width + x].append(ny * width + nx)
                    step_to[(y * width + x, ny * width + nx)] = step


def cell(pos):
    return pos[1] * width + pos[0]


def pos(number):
    return (number % width, number // width)


def toward_nearest(start, points):
    """Where greedy goes from start: a step toward its nearest point."""
    first = {start: None}
    queue = deque([start])
    while queue:
        here = queue.popleft()
        if here in points and here != start:
            return first[here]
        for nxt in adjacent[here]:
            if nxt not in first:
                # Not `first[here] or nxt`: cell 0 is a cell.
                first[nxt] = nxt if here == start else first[here]
                queue.append(nxt)
    return start


def nearest_path(start, points):
    """The cells from start to its nearest point (for drawing)."""
    parent = {start: None}
    queue = deque([start])
    while queue:
        here = queue.popleft()
        if here in points and here != start:
            way = []
            while here is not None:
                way.append(here)
                here = parent[here]
            return way[::-1]
        for nxt in adjacent[here]:
            if nxt not in parent:
                parent[nxt] = here
                queue.append(nxt)
    return [start]


def playout(mine, theirs, points, turns_left, deadline):
    """How the game goes if I step to `mine` and then both players walk
    greedily, up to its end or `turns_left` turns: (lead in half-points,
    turns taken, my first cells), or None when the deadline passes first.
    theirs is None in a solo game, where the lead counts my points."""
    points = set(points)
    their_next = toward_nearest(theirs, points) if theirs is not None else None
    lead, turns, path = 0, 0, []
    while True:
        turns += 1
        if len(path) < 30:
            path.append(mine)
        if mine == their_next and mine in points:
            points.discard(mine)  # split: no change in the lead
        else:
            if mine in points:
                lead += 2
                points.discard(mine)
            if their_next in points:
                lead -= 2
                points.discard(their_next)
        if not points or turns >= turns_left:
            return lead, turns, path
        if time.monotonic() > deadline:
            return None
        mine = toward_nearest(mine, points)
        if their_next is not None:
            their_next = toward_nearest(their_next, points)


def move():
    global calm, rest
    me = cell(dedale.me.pos)
    points = {cell(p) for p in dedale.points}
    theirs = cell(dedale.opponent.pos) if dedale.opponent else None
    greedy = toward_nearest(me, points)
    if me in recent:
        calm = 4
    recent.append(me)
    if calm > 0 or rest > 0 or greedy == me:
        calm, rest = max(0, calm - 1), max(0, rest - 1)
        dedale.path([pos(c) for c in nearest_path(me, points)], label="greedy")
        return step_to.get((me, greedy), Move.STAY)

    # Leave a margin of the move budget for answering.
    deadline = time.monotonic() + dedale.time_left() * 0.6
    turns_left = dedale.max_turns - dedale.turn + 1
    if theirs is not None:
        turns_left = min(HORIZON, turns_left)
    best, best_end = greedy, playout(greedy, theirs, points, turns_left, deadline)
    if best_end is None:
        rest = REST
        return step_to[(me, greedy)]
    greedy_end = best_end
    for nxt in adjacent[me]:
        if nxt == greedy:
            continue
        end = playout(nxt, theirs, points, turns_left, deadline)
        if end is None:
            break
        # A bigger lead first, then an earlier end.
        if (end[0], -end[1]) > (best_end[0], -best_end[1]):
            best, best_end = nxt, end
    if best != greedy:
        dedale.log(f"{summary(best_end)}; greedy's move: {summary(greedy_end)}")
    dedale.path([dedale.me.pos, *(pos(c) for c in best_end[2])], label="playout")
    return step_to[(me, best)]


def summary(end):
    lead, turns, _ = end
    return f"{lead / 2:+g} points in {turns} turns"
