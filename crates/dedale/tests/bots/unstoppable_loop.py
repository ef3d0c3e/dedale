"""Never returns from move() on turn 2, even when the harness stops it."""
import dedale


def move():
    while dedale.turn == 2:
        try:
            while True:
                pass
        except BaseException:  # swallows the stop: nothing gets it out
            pass
    return "stay"
