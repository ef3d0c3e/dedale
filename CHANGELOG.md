# Changelog

## Unreleased

- **Watching in the terminal** (`--watch`, `dedale replay`): marked cells
  get a tinted background instead of a glyph, so a path and marks show on
  the same cell, and points under marks stay visible. Without colours,
  paths take precedence over marked cells.
- **Sandbox**: a Python reached through a symbolic link to its directory,
  as uv installs them, starts in the sandbox instead of crashing every bot.

## 0.1.0 (2026-10-20)

The first release.

- **The game**: one or two bots in a maze with 180° symmetry, from
  opposite corners. Solo, clear it in the fewest turns; duo, collect more
  points than the opponent. Moves are simultaneous, a point reached
  together is split, and every bot gets a startup budget and a time limit
  per move; a move that runs late costs that turn only.
- **Mazes** from a printed code, so any game can be played again
  (`--maze`), with a chosen size, share of loops and number of points;
  map files too.
- **Python bots** through a harness built into Dedale (`init()`, `move()`,
  the `dedale` module), with logs, marks and paths drawn on the maze. Any
  other language through a JSON-lines protocol.
- **Bots say who they are**: name, author, description and version, read
  without running them; `dedale about` prints them as JSON.
- **Replays** with every turn's timing, outcome, log and marks;
  `dedale replay` rewinds and verifies them. **Tournaments** with a
  leaderboard.
- **A browser app**, built in (`dedale view`, `--view`): the maze, a
  timeline with every problem marked, a turn inspector, either or both
  players' marks, heatmaps, and charts of latency, time in `move()` and
  score (legends with avg / 5% / 1%). Games and tournaments can be watched
  live.
- **A sandbox** for bots you did not write (`--sandbox`, Linux): bubblewrap,
  memory and process limits, a CPU per bot.
- **Example bots** (greedy, tour, rollout) and a guide to writing a bot.
- **Prebuilt binaries** for Linux (x86_64 and arm64, static) and macOS
  (Apple silicon and Intel).
