"""init() raises; move() still works."""


def init():
    raise RuntimeError("init failed on purpose")


def move():
    return "stay"
