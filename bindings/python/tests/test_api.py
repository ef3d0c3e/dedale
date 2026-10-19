"""The dedale module's types and helpers."""

import unittest

import dedale
from dedale import Maze, Move, Player
from dedale._harness import MAX_LOG_BYTES, TRUNCATED, Sink, cap_log, normalize

# A corridor ring around a block of walls, 14 points.
RING = "_.....###..###..###....._"


class MoveTests(unittest.TestCase):
    def test_values(self):
        self.assertIs(Move("up"), Move.UP)
        self.assertEqual(Move.LEFT, "left")
        self.assertEqual(str(Move.RIGHT), "right")
        self.assertEqual(Move.UP.delta, (0, -1))
        self.assertEqual(Move.STAY.delta, (0, 0))


class MazeTests(unittest.TestCase):
    def setUp(self):
        self.maze = Maze(5, 5, RING)

    def test_rows(self):
        self.assertEqual(self.maze.rows, ("_____", "_###_", "_###_", "_###_", "_____"))

    def test_walls_and_bounds(self):
        self.assertTrue(self.maze.is_open(0, 0))
        self.assertTrue(self.maze.is_wall(1, 1))
        self.assertTrue(self.maze.is_wall(-1, 0))
        self.assertTrue(self.maze.is_wall(5, 0))
        self.assertFalse(self.maze.in_bounds(0, 5))

    def test_neighbors(self):
        self.assertEqual(
            self.maze.neighbors(0, 0), [(Move.DOWN, (0, 1)), (Move.RIGHT, (1, 0))]
        )
        self.assertEqual(self.maze.neighbors(2, 0), [(Move.LEFT, (1, 0)), (Move.RIGHT, (3, 0))])


class MarkTests(unittest.TestCase):
    def setUp(self):
        dedale.maze = Maze(5, 5, RING)
        dedale._take_marks()

    def tearDown(self):
        dedale._take_marks()

    def test_marks_drop_cells_outside_the_maze(self):
        dedale.mark([(0, 0), (4, 4), (5, 0), (-1, 2)], label="path", color="red")
        marks, truncated = dedale._take_marks()
        self.assertEqual(marks, [{"cells": [[0, 0], [4, 4]], "label": "path", "color": "red"}])
        self.assertFalse(truncated)
        self.assertEqual(dedale._take_marks(), ([], False))

    def test_mark_caps(self):
        for _ in range(dedale.MAX_MARKS + 5):
            dedale.mark([(0, 0)] * 100, label="é" * 100)
        marks, truncated = dedale._take_marks()
        self.assertTrue(truncated)
        self.assertEqual(len(marks), 41)
        self.assertEqual(sum(len(m["cells"]) for m in marks), dedale.MAX_MARK_CELLS)
        self.assertTrue(all(len(m["label"].encode()) <= dedale.MAX_LABEL for m in marks))


class PlayerTests(unittest.TestCase):
    def test_defaults(self):
        player = Player(1, (4, 4))
        self.assertEqual((player.id, player.pos, player.score, player.last_move), (1, (4, 4), 0.0, None))


class CapLogTests(unittest.TestCase):
    def test_small_logs_are_kept(self):
        self.assertEqual(cap_log(["a", "b"], False), ["a", "b"])
        self.assertEqual(cap_log(["a"], True), ["a", TRUNCATED])

    def test_large_logs_are_cut(self):
        for entries in (["x" * 5000], ["é" * 3000, "lost"], ["a" * 4090, "b" * 10]):
            capped = cap_log(entries, False)
            self.assertLessEqual(sum(len(e.encode()) for e in capped), MAX_LOG_BYTES)
            self.assertEqual(capped[-1], TRUNCATED)
            self.assertEqual(cap_log(capped, False), capped)

    def test_sink_is_bounded(self):
        sink = Sink()
        sink.write("line one\nline two\n")
        self.assertEqual(sink.lines(), ["line one", "line two"])
        sink.write("y" * 100_000)
        self.assertTrue(sink.overflow)
        self.assertLess(sink.size, 10_000)


class NormalizeTests(unittest.TestCase):
    def test_moves_and_strings(self):
        self.assertEqual(normalize(Move.DOWN), ("down", None))
        self.assertEqual(normalize("LEFT"), ("left", None))
        self.assertEqual(normalize("north"), ("north", None))

    def test_other_values_are_explained(self):
        for value in (None, 42, ["up"]):
            move, problem = normalize(value)
            self.assertIsNone(move)
            self.assertIn(repr(value), problem)


if __name__ == "__main__":
    unittest.main()
