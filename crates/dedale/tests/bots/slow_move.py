"""Thinks for 300 ms on turn 3."""
import time

import dedale


def move():
    if dedale.turn == 3:
        time.sleep(0.3)
    return "stay"
