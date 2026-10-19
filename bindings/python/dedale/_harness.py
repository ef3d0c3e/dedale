"""Runs a Python bot for Dedale.

Usage: python -u _harness.py <bot.py>

The protocol uses private copies of stdin and stdout: the bot's own stdin
reads nothing, and anything it writes to stdout (``print`` or the raw file
descriptor) ends up in its log or on stderr, never in the protocol.
"""

from __future__ import annotations

import importlib.util
import io
import json
import os
import select
import sys
import time
import traceback
from pathlib import Path

if not __package__:
    # Run as a script, sys.path[0] is this file's directory; the package root
    # is its parent.
    sys.path[0] = str(Path(__file__).resolve().parent.parent)

import dedale  # noqa: E402
from dedale import Maze, Move, Player  # noqa: E402

PROTOCOL = 1
MAX_LOG_BYTES = 4096
MAX_ERROR_TEXT = 4096
TRUNCATED = "[truncated]"
# Characters kept by the capture buffer: a bound on memory, above the byte
# cap applied when the call ends.
SINK_LIMIT = 2 * MAX_LOG_BYTES


class Sink(io.TextIOBase):
    """Stands in for ``sys.stdout`` during a call, keeping a bounded prefix."""

    def __init__(self) -> None:
        self.parts: list[str] = []
        self.size = 0
        self.overflow = False

    def writable(self) -> bool:
        return True

    def write(self, text: str) -> int:
        room = SINK_LIMIT - self.size
        if len(text) > room:
            self.overflow = True
            text = text[:room]
        self.parts.append(text)
        self.size += len(text)
        return len(text)

    def lines(self) -> list[str]:
        text = "".join(self.parts)
        if text.endswith("\n"):
            text = text[:-1]
        return text.split("\n") if text else []


def cap_bytes(text: str, limit: int) -> str:
    return text.encode()[:limit].decode(errors="ignore")


def cap_log(entries: list[str], truncated: bool) -> list[str]:
    """At most MAX_LOG_BYTES, the marker included, as Dedale enforces."""
    if truncated:
        entries = [*entries, TRUNCATED]
    if sum(len(e.encode()) for e in entries) <= MAX_LOG_BYTES:
        return entries
    budget = MAX_LOG_BYTES - len(TRUNCATED)
    kept = []
    for entry in entries:
        size = len(entry.encode())
        if size > budget:
            kept.append(cap_bytes(entry, budget))
            break
        kept.append(entry)
        budget -= size
    kept.append(TRUNCATED)
    return kept


class Capture:
    """Collects what a call prints and marks."""

    def __enter__(self) -> Capture:
        self.sink = Sink()
        sys.stdout = self.sink
        dedale._take_marks()
        return self

    def __exit__(self, *exc: object) -> None:
        sys.stdout = sys.stderr
        marks, marks_truncated = dedale._take_marks()
        self.marks = marks
        self.log = cap_log(self.sink.lines(), self.sink.overflow or marks_truncated)


def error_info(exc: BaseException) -> dict:
    message = f"{type(exc).__name__}: {exc}"
    return {
        "message": cap_bytes(message, MAX_ERROR_TEXT),
        "traceback": cap_bytes(traceback.format_exc(), MAX_ERROR_TEXT),
    }


class Reader:
    """Reads protocol lines from a file descriptor, blocking or not."""

    def __init__(self, fd: int) -> None:
        self.fd = fd
        self.buffer = b""
        self.eof = False

    def fill(self, timeout: float | None) -> bool:
        ready, _, _ = select.select([self.fd], [], [], timeout)
        if not ready:
            return False
        chunk = os.read(self.fd, 1 << 16)
        if chunk:
            self.buffer += chunk
        else:
            self.eof = True
        return bool(chunk)

    def line(self) -> bytes | None:
        """The next line, waiting for it; ``None`` at end of input."""
        while b"\n" not in self.buffer:
            if self.eof:
                return None
            self.fill(None)
        line, self.buffer = self.buffer.split(b"\n", 1)
        return line

    def pending(self) -> list[bytes]:
        """Every complete line already available, without waiting."""
        while not self.eof and self.fill(0):
            pass
        *lines, self.buffer = self.buffer.split(b"\n")
        return lines


class Harness:
    def __init__(self, bot_path: str, reader: Reader, out: io.TextIOBase) -> None:
        self.bot_path = bot_path
        self.reader = reader
        self.out = out
        self.move_fn = None

    def send(self, msg: dict) -> None:
        self.out.write(json.dumps(msg, ensure_ascii=False, separators=(",", ":")) + "\n")
        self.out.flush()

    def run(self) -> None:
        self.send({"type": "hello", "protocol": PROTOCOL})
        init = self.next_message()
        if init is None or init.get("type") != "init":
            return
        self.start(init)
        while True:
            line = self.reader.line()
            if line is None:
                return
            latest = None
            for raw in [line, *self.reader.pending()]:
                msg = json.loads(raw)
                if msg["type"] == "turn":
                    apply_turn(msg)
                    latest = msg
                elif msg["type"] == "end":
                    return
            if latest is not None:
                self.play(latest["turn"])

    def next_message(self) -> dict | None:
        line = self.reader.line()
        return None if line is None else json.loads(line)

    def start(self, init: dict) -> None:
        received = time.perf_counter()
        apply_init(init)
        dedale._deadline = received + dedale.startup_timeout
        error = None
        started = time.perf_counter_ns()
        with Capture() as capture:
            try:
                module = load(self.bot_path)
                # Taken before init(), so a failing init() still plays.
                move_fn = getattr(module, "move", None)
                self.move_fn = move_fn if callable(move_fn) else None
                if callable(getattr(module, "init", None)):
                    module.init()
                if self.move_fn is None:
                    raise AttributeError("the bot defines no move() function")
            except Exception as exc:  # reported to Dedale; SystemExit still exits
                error = error_info(exc)
        init_us = (time.perf_counter_ns() - started) // 1000
        if error is not None:
            self.send({"type": "error", "phase": "init", **error, "log": capture.log})
        else:
            self.send(
                {"type": "ready", "init_us": init_us, "log": capture.log, "marks": capture.marks}
            )

    def play(self, turn: int) -> None:
        dedale._deadline = time.perf_counter() + dedale.move_timeout
        result, error = None, None
        started = time.perf_counter_ns()
        with Capture() as capture:
            try:
                if self.move_fn is not None:
                    result = self.move_fn()
            except Exception as exc:  # reported to Dedale; SystemExit still exits
                error = error_info(exc)
        think_us = (time.perf_counter_ns() - started) // 1000
        move = None
        log = capture.log
        if self.move_fn is None:
            log = cap_log([*log, "[dedale] the bot has no move() (see its startup error)"], False)
        elif error is None:
            move, problem = normalize(result)
            if problem:
                log = cap_log([*log, problem], False)
        self.send(
            {
                "type": "move",
                "turn": turn,
                "move": move,
                "think_us": think_us,
                "log": log,
                "marks": capture.marks,
                "error": error,
            }
        )


def normalize(result: object) -> tuple[str | None, str | None]:
    """The move to send, and a log entry explaining a rejected return value."""
    if isinstance(result, Move):
        return result.value, None
    if isinstance(result, str):
        # Unknown names go through as they are: Dedale reports them.
        return result.lower(), None
    expected = "expected a Move or a move name"
    return None, f"[dedale] move() returned {result!r}; {expected}"


def load(path: str):
    """Imports the bot from its file, with its directory importable."""
    bot = Path(path).resolve()
    sys.path.insert(0, str(bot.parent))
    spec = importlib.util.spec_from_file_location("dedale_bot", bot)
    if spec is None or spec.loader is None:
        raise ImportError(f"cannot load {bot}")
    module = importlib.util.module_from_spec(spec)
    sys.modules["dedale_bot"] = module
    spec.loader.exec_module(module)
    return module


def apply_init(msg: dict) -> None:
    width, height, cells = msg["width"], msg["height"], msg["cells"]
    dedale.maze = Maze(width, height, cells)
    dedale.points = {(i % width, i // width) for i, c in enumerate(cells) if c == "."}
    dedale.players = [Player(i, tuple(p["pos"])) for i, p in enumerate(msg["players"])]
    dedale.me = dedale.players[msg["you"]]
    others = [p for p in dedale.players if p is not dedale.me]
    dedale.opponent = others[0] if others else None
    dedale.turn = 0
    dedale.max_turns = msg["max_turns"]
    dedale.move_timeout = msg["move_ms"] / 1000
    dedale.startup_timeout = msg["startup_ms"] / 1000
    dedale.code = msg.get("code")


def apply_turn(msg: dict) -> None:
    dedale.turn = msg["turn"]
    for player, state in zip(dedale.players, msg["players"]):
        player.pos = tuple(state["pos"])
        player.score = state["half_points"] / 2
        player.last_move = Move(state["last_move"]) if state["last_move"] else None
    dedale.points.difference_update(tuple(c) for c in msg["collected"])


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        print("usage: _harness.py <bot.py>", file=sys.stderr)
        return 2
    # Keep the protocol pipes for ourselves: fd 0 becomes /dev/null and fd 1
    # a copy of stderr.
    proto_in = os.dup(0)
    proto_out = os.fdopen(os.dup(1), "w", encoding="utf-8")
    devnull = os.open(os.devnull, os.O_RDONLY)
    os.dup2(devnull, 0)
    os.close(devnull)
    os.dup2(2, 1)
    sys.stdin = io.StringIO("")
    sys.stdout = sys.stderr
    try:
        Harness(argv[1], Reader(proto_in), proto_out).run()
    except BrokenPipeError:
        pass
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
