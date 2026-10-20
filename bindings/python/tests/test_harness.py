"""The harness driven through its pipes, the way Dedale drives it."""

import json
import os
import queue
import subprocess
import sys
import tempfile
import textwrap
import threading
import unittest
from pathlib import Path

HARNESS = Path(__file__).resolve().parent.parent / "dedale" / "_harness.py"

# A corridor ring around a block of walls, 14 points.
RING = "_.....###..###..###....._"


def init_msg(**fields):
    msg = {
        "type": "init",
        "protocol": 1,
        "code": None,
        "width": 5,
        "height": 5,
        "cells": RING,
        "you": 0,
        "players": [{"pos": [0, 0]}],
        "startup_ms": 2000,
        "move_ms": 100,
        "max_turns": 100,
    }
    msg.update(fields)
    return msg


def turn_msg(turn, pos=(0, 0), collected=(), half_points=0, last_move=None):
    return {
        "type": "turn",
        "turn": turn,
        "players": [{"pos": list(pos), "half_points": half_points, "last_move": last_move}],
        "collected": [list(c) for c in collected],
    }


END = {"type": "end", "reason": "cleared", "turn": 1, "half_points": [0]}


class Bot:
    """The harness running a bot written from source."""

    def __init__(self, source, **modules):
        self.dir = tempfile.TemporaryDirectory()
        for name, text in modules.items():
            (Path(self.dir.name) / f"{name}.py").write_text(textwrap.dedent(text))
        path = Path(self.dir.name) / "bot.py"
        path.write_text(textwrap.dedent(source))
        env = dict(os.environ, PYTHONDONTWRITEBYTECODE="1")
        self.proc = subprocess.Popen(
            [sys.executable, "-u", str(HARNESS), str(path)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=env,
        )
        self.lines = queue.Queue()
        threading.Thread(target=self._read, daemon=True).start()

    def _read(self):
        for line in self.proc.stdout:
            self.lines.put(line)
        self.lines.put(None)

    def send(self, msg):
        self.proc.stdin.write((json.dumps(msg) + "\n").encode())
        self.proc.stdin.flush()

    def recv(self, timeout=5.0):
        line = self.lines.get(timeout=timeout)
        if line is None:
            raise AssertionError(f"harness exited: {self.stderr()}")
        return json.loads(line)

    def nothing_within(self, seconds):
        try:
            line = self.lines.get(timeout=seconds)
        except queue.Empty:
            return True
        raise AssertionError(f"unexpected output: {line!r}")

    def start(self, **init_fields):
        hello = self.recv()
        assert hello == {"type": "hello", "protocol": 1}, hello
        self.send(init_msg(**init_fields))
        return self.recv()

    def wait(self, timeout=5.0):
        return self.proc.wait(timeout=timeout)

    def stderr(self):
        if self.proc.poll() is None:
            return "(still running)"
        return self.proc.stderr.read().decode(errors="replace")

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        if self.proc.poll() is None:
            self.proc.kill()
        self.proc.wait()
        for stream in (self.proc.stdin, self.proc.stdout, self.proc.stderr):
            stream.close()
        self.dir.cleanup()


class HarnessTests(unittest.TestCase):
    def test_a_whole_game(self):
        source = """
            import dedale
            from dedale import Move

            print("imported", dedale.maze.width, len(dedale.points))

            def init():
                print("init", dedale.me.pos, dedale.opponent)

            def move():
                me = dedale.me
                print("turn", dedale.turn, me.pos, me.score, me.last_move, len(dedale.points))
                return Move.RIGHT
        """
        with Bot(source) as bot:
            ready = bot.start()
            self.assertEqual(ready["type"], "ready")
            self.assertIsInstance(ready["init_us"], int)
            self.assertEqual(ready["log"], ["imported 5 14", "init (0, 0) None"])

            bot.send(turn_msg(1))
            reply = bot.recv()
            self.assertEqual((reply["type"], reply["turn"], reply["move"]), ("move", 1, "right"))
            self.assertEqual(reply["log"], ["turn 1 (0, 0) 0.0 None 14"])
            self.assertIsInstance(reply["think_us"], int)
            self.assertIsNone(reply["error"])

            bot.send(turn_msg(2, pos=(1, 0), collected=[(1, 0)], half_points=2, last_move="right"))
            reply = bot.recv()
            self.assertEqual(reply["log"], ["turn 2 (1, 0) 1.0 right 13"])

            bot.send(END)
            self.assertEqual(bot.wait(), 0)

    def test_queued_turns_are_skipped(self):
        source = """
            import time
            import dedale

            def init():
                time.sleep(0.3)

            def move():
                print(dedale.turn, len(dedale.points))
                return "stay"
        """
        with Bot(source) as bot:
            self.assertEqual(bot.recv()["type"], "hello")
            bot.send(init_msg())
            bot.send(turn_msg(1))
            bot.send(turn_msg(2, collected=[(1, 0)]))
            bot.send(turn_msg(3, collected=[(2, 0)]))
            self.assertEqual(bot.recv()["type"], "ready")
            reply = bot.recv()
            self.assertEqual((reply["turn"], reply["log"]), (3, ["3 12"]))
            self.assertTrue(bot.nothing_within(0.3))

    def test_return_values(self):
        source = """
            import dedale
            from dedale import Move

            RETURNS = [Move.LEFT, "UP", None, 42, "north"]

            def move():
                return RETURNS[dedale.turn - 1]
        """
        with Bot(source) as bot:
            bot.start()
            replies = []
            for t in range(1, 6):
                bot.send(turn_msg(t))
                replies.append(bot.recv())
            self.assertEqual([r["move"] for r in replies], ["left", "up", None, None, "north"])
            self.assertIn("None", replies[2]["log"][-1])
            self.assertIn("42", replies[3]["log"][-1])
            self.assertTrue(all(r["error"] is None for r in replies))

    def test_an_exception_in_move(self):
        source = """
            def move():
                raise ValueError("boom")
        """
        with Bot(source) as bot:
            bot.start()
            bot.send(turn_msg(1))
            reply = bot.recv()
            self.assertIsNone(reply["move"])
            self.assertEqual(reply["error"]["message"], "ValueError: boom")
            self.assertIn("boom", reply["error"]["traceback"])

    def test_an_exception_in_init_still_plays(self):
        source = """
            def init():
                print("about to fail")
                raise RuntimeError("nope")

            def move():
                return "stay"
        """
        with Bot(source) as bot:
            reply = bot.start()
            self.assertEqual((reply["type"], reply["phase"]), ("error", "init"))
            self.assertEqual(reply["message"], "RuntimeError: nope")
            self.assertIn("nope", reply["traceback"])
            self.assertEqual(reply["log"], ["about to fail"])
            bot.send(turn_msg(1))
            self.assertEqual(bot.recv()["move"], "stay")

    def test_a_bot_without_move(self):
        with Bot("x = 1\n") as bot:
            reply = bot.start()
            self.assertEqual(reply["type"], "error")
            self.assertIn("move()", reply["message"])
            bot.send(turn_msg(1))
            reply = bot.recv()
            self.assertIsNone(reply["move"])
            self.assertIn("no move()", reply["log"][-1])

    def test_a_syntax_error(self):
        with Bot("def move(:\n") as bot:
            reply = bot.start()
            self.assertEqual(reply["type"], "error")
            self.assertIn("SyntaxError", reply["message"])

    def test_logs_are_captured_and_capped(self):
        source = """
            import dedale

            def move():
                dedale.log("value", 1, sep="=")
                print("x" * 10000)
                return "stay"
        """
        with Bot(source) as bot:
            bot.start()
            bot.send(turn_msg(1))
            log = bot.recv()["log"]
            self.assertEqual(log[0], "value=1")
            self.assertEqual(log[-1], "[truncated]")
            self.assertLessEqual(sum(len(e.encode()) for e in log), 4096)

    def test_marks(self):
        source = """
            import dedale

            def move():
                dedale.mark([(0, 0), (1, 0), (99, 99), (-1, 0)], label="path")
                dedale.path([(0, 0), (1, 0)], color="blue")
                for _ in range(100):
                    dedale.mark([(2, 0)])
                return "stay"
        """
        with Bot(source) as bot:
            bot.start()
            bot.send(turn_msg(1))
            reply = bot.recv()
            marks = reply["marks"]
            self.assertEqual(len(marks), 64)
            self.assertEqual(marks[0], {"cells": [[0, 0], [1, 0]], "label": "path", "color": None, "kind": "cells"})
            self.assertEqual(marks[1], {"cells": [[0, 0], [1, 0]], "label": None, "color": "blue", "kind": "path"})
            self.assertEqual(reply["log"], ["[truncated]"])

    def test_raw_stdout_and_stdin_stay_private(self):
        source = """
            import os
            import sys

            def move():
                os.write(1, b"raw garbage\\n")
                sys.__stdout__.write("more garbage\\n")
                sys.__stdout__.flush()
                try:
                    input()
                except EOFError:
                    print("stdin is empty")
                return "stay"
        """
        with Bot(source) as bot:
            bot.start()
            bot.send(turn_msg(1))
            reply = bot.recv()
            self.assertEqual(reply["move"], "stay")
            self.assertEqual(reply["log"], ["stdin is empty"])
            bot.send(END)
            bot.wait()
            stderr = bot.stderr()
            self.assertIn("raw garbage", stderr)
            self.assertIn("more garbage", stderr)

    def test_time_left(self):
        source = """
            import dedale

            def init():
                print(0 < dedale.time_left() <= 2.0)

            def move():
                print(0 < dedale.time_left() <= 0.1)
                return "stay"
        """
        with Bot(source) as bot:
            self.assertEqual(bot.start()["log"], ["True"])
            bot.send(turn_msg(1))
            self.assertEqual(bot.recv()["log"], ["True"])

    def test_sys_exit_ends_the_process(self):
        source = """
            import sys

            def move():
                sys.exit(5)
        """
        with Bot(source) as bot:
            bot.start()
            bot.send(turn_msg(1))
            self.assertEqual(bot.wait(), 5)

    def test_end_of_input_exits(self):
        with Bot("def move():\n    return 'stay'\n") as bot:
            self.assertEqual(bot.recv()["type"], "hello")
            bot.proc.stdin.close()
            self.assertEqual(bot.wait(), 0)

    def test_sibling_modules_are_importable(self):
        source = """
            from helper import direction

            def move():
                return direction()
        """
        helper = """
            def direction():
                return "down"
        """
        with Bot(source, helper=helper) as bot:
            bot.start()
            bot.send(turn_msg(1))
            self.assertEqual(bot.recv()["move"], "down")

    def test_duo_state(self):
        source = """
            import dedale

            def init():
                print(dedale.me.id, dedale.opponent.id, dedale.opponent.pos)

            def move():
                print(dedale.opponent.pos, dedale.opponent.score)
                return "stay"
        """
        players = [{"pos": [0, 0]}, {"pos": [4, 4]}]
        with Bot(source) as bot:
            self.assertEqual(bot.start(you=1, players=players)["log"], ["1 0 (0, 0)"])
            turn = turn_msg(1)
            turn["players"] = [
                {"pos": [1, 0], "half_points": 3, "last_move": "right"},
                {"pos": [4, 4], "half_points": 0, "last_move": None},
            ]
            bot.send(turn)
            self.assertEqual(bot.recv()["log"], ["(1, 0) 1.5"])


if __name__ == "__main__":
    unittest.main()
