"""Returns things that are not moves."""
import dedale

RETURNS = ["stay", None, 42, "north", "DOWN"]


def move():
    return RETURNS[(dedale.turn - 1) % len(RETURNS)]
