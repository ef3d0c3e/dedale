"""Exits with status 7 on turn 3."""
import sys

import dedale


def move():
    if dedale.turn == 3:
        sys.exit(7)
    return "stay"
