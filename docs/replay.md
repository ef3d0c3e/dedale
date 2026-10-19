# Replay and tournament files

## `replay.json`

One JSON object per game (format 1), written by `dedale run` and by every
game of a tournament. `dedale replay --verify` checks one against the rules.

| Field | |
|---|---|
| `format` | 1 |
| `dedale` | Version of Dedale that played the game |
| `code` | Maze code, `null` for a map file |
| `maze` | `width`, `height`, `cells`: the maze at turn 0, as in the protocol |
| `config` | `startup_ms`, `move_ms`, `max_turns`, `seating` (`solo`, `duo`, `duo_swapped`) |
| `players` | Per bot: `name`, `path`, `start`, `startup`, `exit` |
| `turns` | One entry per turn: `turn`, `players`, `collected` |
| `events` | Out-of-turn happenings: `{turn, player, kind: "late", for_turn}` or `{turn, player, kind: "ignored", reason}` |
| `result` | `reason` (`cleared`, `turn_limit`, `abandoned`), `turns`, `half_points`, `winner` (`null` on a draw and in solo), `cleared_turn` |
| `stats` | Per bot, see below |

`startup`: `outcome` (`ok`, `late`, `init_error`, `timeout`, `crashed`),
`startup_us` (measured by Dedale), `init_us` (measured by the bot), `log`,
`marks`, `error`.

`exit`: `code`, `signal`, `killed` (Dedale had to kill it), `stderr_tail`.

A turn's `players` entries: `requested` and `applied` moves, `outcome` (see
the [protocol](protocol.md#outcomes)), `latency_us` (Dedale's measure, from
sending the turn to reading the answer), `think_us` (the bot's), `pos` after
the turn, `gained` and `half_points`, `log`, `marks`, `error`, and `invalid`
(why an answer was rejected).

The state after turn *t* is the initial maze minus every `collected` cell up
to *t*, with the players at the positions stored in turn *t*: rewinding needs
no simulation.

`stats`, per bot: `startup_us`, `init_us`, `latency_us` and `think_us`
summaries (`count`, `min`, `mean`, `p50`, `p95`, `p99`, `max`; nearest-rank
percentiles, `null` without samples), `outcomes` (a count per outcome), `late`,
`turns`, `steps` (legal moves other than stay), `stays` (stays asked for),
`half_points`, `turns_to_clear` (cleared solo games). All integers, in
microseconds where timed.

## `tournament.json`

Written by `dedale tournament` (format 1).

| Field | |
|---|---|
| `master_seed` | 16 hex digits; every maze code derives from it |
| `mode` | `duo` or `solo` |
| `maze` | `{"generated": {width, height, loops, points}}` (`points` as asked for; 65535 means every open cell) or `{"map": {path}}` |
| `startup_ms`, `move_ms`, `max_turns` | Timing |
| `bots` | `name`, `path` |
| `games` | `index`, `code`, `seating`, `bots` (who played, in player order), `result`, `replay` (path relative to this file) |
| `leaderboard` | Per bot, best first: `rank` (shared on ties), `games`, `match_points`, `wins`, `draws`, `losses`, `cleared`, `turns_to_clear`, `half_points`, `failures` (turns with any outcome but `ok`), `latency_us` |
