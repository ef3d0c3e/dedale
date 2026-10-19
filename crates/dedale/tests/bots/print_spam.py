"""Prints far more than the log keeps."""


def move():
    for i in range(10_000):
        print("spam", i)
    return "stay"
