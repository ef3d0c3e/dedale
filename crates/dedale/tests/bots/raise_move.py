"""move() raises on turn 2."""
import dedale


def move():
    if dedale.turn == 2:
        raise ValueError("move failed on purpose")
    return "stay"
