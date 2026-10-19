"""Never returns from move() on turn 2."""
import dedale


def move():
    while dedale.turn == 2:
        pass
    return "stay"
