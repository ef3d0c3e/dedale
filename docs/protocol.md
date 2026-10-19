# Bot protocol

How Dedale talks to a competitor program. Python bots get this for free from
the embedded harness; bots in other languages implement it directly. Dedale
runs any bot path that does not end in `.py` as an executable.

## Transport

- Dedale writes to the bot's **stdin** and reads its **stdout**: UTF-8, one
  JSON object per line.
- Lines from the bot may be at most 64 KiB; a longer one is discarded and
  counts as invalid.
- **stderr** is free for the bot: it goes to `<name>.stderr.log`.
- The bot runs in its own process group, which Dedale kills at the end of the
  game.
- Every message has a `"type"`. Unknown fields are ignored.

## Sequence

```
bot     → hello                 as soon as the program starts
dedale  → init                  the startup budget starts now
bot     → ready (or error)      startup done
dedale  → turn 1                the move budget starts now
bot     → move 1
  …
dedale  → end                   exit now; killed after 1 s
```

- `hello` must arrive within 10 s, or the bot is out for the whole game.
- Turns are sent even before `ready`; until then they count as "not ready".
  After a slow startup, answer only the newest turn.
- An answer must carry the turn it answers. Answers for turns that already
  passed are recorded as late and ignored.

## Messages from Dedale

`init`:

```json
{"type": "init", "protocol": 1, "code": "ded2-31x31-l25-p534-9f3a0c11d2e4b6a8",
 "width": 31, "height": 31, "cells": "_.....",
 "you": 0, "players": [{"pos": [0, 0]}, {"pos": [30, 30]}],
 "startup_ms": 2000, "move_ms": 100, "max_turns": 3844}
```

`cells` holds `width × height` characters, row by row: `#` wall, `.` point,
`_` empty. `code` is `null` for a map file. `you` is your index in `players`
(which has one entry in solo).

`turn`, the state after the previous turn:

```json
{"type": "turn", "turn": 1,
 "players": [{"pos": [0, 0], "half_points": 0, "last_move": null},
             {"pos": [30, 30], "half_points": 0, "last_move": null}],
 "collected": []}
```

`collected` lists the cells whose point was taken during the previous turn.
Scores are in half-points (a point is worth 2; a point both players reach in
the same turn gives each 1). `last_move` is the move actually applied (`stay`
when it was rejected).

`end`:

```json
{"type": "end", "reason": "cleared", "turn": 812, "half_points": [700, 692]}
```

`reason` is `cleared`, `turn_limit` or `abandoned`.

## Messages from the bot

```json
{"type": "hello", "protocol": 1}
{"type": "ready", "init_us": 39870, "log": [], "marks": []}
{"type": "error", "phase": "init", "message": "ValueError: ...", "traceback": "...", "log": []}
{"type": "move", "turn": 1, "move": "up", "think_us": 812,
 "log": ["target (5,3)"], "marks": [{"cells": [[1, 0], [2, 0]], "label": "path", "color": null}],
 "error": null}
```

- `move` is `up`, `down`, `left`, `right` or `stay`, in lowercase; `null`
  with an `error` object (`message`, `traceback`) when the bot failed.
- `init_us` and `think_us` (optional) are the bot's own measure of its
  startup and of its move, in microseconds; Dedale records them next to its
  own latency measurements.
- `log`, `marks` and `error` are optional. Dedale keeps 4 KiB of log, 64 marks,
  4096 marked cells and 64-byte labels per message, and caps error texts at
  4 KiB.

## Outcomes

Every turn, each bot gets one outcome: `ok`, `illegal` (wall or off the grid),
`invalid` (unreadable answer, unknown move), `error` (the bot reported a
failure), `timeout`, `not_ready` (still starting) or `crashed` (the process is
gone). All but `ok` resolve as `stay`.
