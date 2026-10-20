// What the run browser and the tournament view list, as plain strings.

import { describeResult, formatSpread, letter, score } from "./format.ts";
import type {
  GameSummary,
  Listing,
  Mode,
  Seating,
  Standing,
  TournamentReport,
  TournamentSummary,
} from "./types.ts";

export type Entry = Listing["entries"][number];

/** Newest first, then by path. */
export function sortEntries(entries: readonly Entry[]): Entry[] {
  return [...entries].sort((a, b) => b.modified - a.modified || a.path.localeCompare(b.path));
}

/** The entries with a bot whose name holds `query` (any case); all for an empty query. */
export function filterEntries(entries: readonly Entry[], query: string): Entry[] {
  const needle = query.trim().toLowerCase();
  if (!needle) return [...entries];
  return entries.filter((e) => e.bots.some((bot) => bot.toLowerCase().includes(needle)));
}

/** `2026-10-19 21:12`, local time. */
export function formatTime(seconds: number): string {
  const d = new Date(seconds * 1000);
  const two = (n: number): string => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${two(d.getMonth() + 1)}-${two(d.getDate())} ${two(d.getHours())}:${two(d.getMinutes())}`;
}

export function seatingName(seating: Seating | null): string {
  switch (seating) {
    case "solo":
      return "solo";
    case "duo":
      return "duo";
    case "duo_swapped":
      return "duo, corners swapped";
    default:
      return "-";
  }
}

/** How a listed game went. */
export function gameOutcome(game: Omit<GameSummary, "kind">): string {
  if (game.error) return game.error;
  if (!game.result || !game.seating) return "-";
  return describeResult(game.result, game.seating, game.bots);
}

/** How a listed tournament went. */
export function tournamentOutcome(t: TournamentSummary): string {
  if (t.error) return t.error;
  const games = `${t.games.length} ${t.games.length === 1 ? "game" : "games"}`;
  if (t.leaders.length === 0) return games;
  return `${games}, ${t.leaders.length > 1 ? "tied first" : "won by"} ${t.leaders.join(", ")}`;
}

/** The directory holding a file at `path` (`a/b/tournament.json` → `a/b`). */
export function dirOf(path: string): string {
  const slash = path.lastIndexOf("/");
  return slash < 0 ? "" : path.slice(0, slash);
}

/** A tournament game's replay, relative to the root. */
export function replayPath(tournamentPath: string, replay: string): string {
  const dir = dirOf(tournamentPath);
  return dir ? `${dir}/${replay}` : replay;
}

export interface Table {
  headers: string[];
  /** Columns holding numbers, aligned right. */
  numeric: number[];
  rows: string[][];
}

/** The leaderboard of a mode, as the terminal prints it. */
export function leaderboard(mode: Mode, standings: readonly Standing[]): Table {
  const latency = (s: Standing): string => formatSpread(s.latency_us);
  if (mode === "duo") {
    return {
      headers: [
        "rank",
        "bot",
        "points",
        "won",
        "drawn",
        "lost",
        "score",
        "failures",
        "latency avg / 5% / 1%",
      ],
      numeric: [0, 2, 3, 4, 5, 6, 7, 8],
      rows: standings.map((s) => [
        String(s.rank),
        s.name,
        String(s.match_points),
        String(s.wins),
        String(s.draws),
        String(s.losses),
        score(s.half_points),
        String(s.failures),
        latency(s),
      ]),
    };
  }
  return {
    headers: ["rank", "bot", "cleared", "turns", "score", "failures", "latency avg / 5% / 1%"],
    numeric: [0, 2, 3, 4, 5, 6],
    rows: standings.map((s) => [
      String(s.rank),
      s.name,
      `${s.cleared}/${s.games}`,
      String(s.turns_to_clear),
      score(s.half_points),
      String(s.failures),
      latency(s),
    ]),
  };
}

/** A tournament's settings in a line. */
export function settings(report: TournamentReport): string {
  const maze =
    "generated" in report.maze
      ? `${report.maze.generated.width}×${report.maze.generated.height} mazes, ${report.maze.generated.loops}% loops`
      : `map ${report.maze.map.path}`;
  const turns = report.max_turns === null ? "" : `, ${report.max_turns} turns at most`;
  return `${maze}, ${report.startup_ms} ms startup, ${report.move_ms} ms per move${turns}, master seed ${report.master_seed}`;
}

/** Who plays a tournament game, by letter: `A lap, B sitter`. */
export function players(bots: readonly number[], names: readonly string[]): string {
  return bots.map((b, i) => `${letter(i)} ${names[b] ?? "?"}`).join(", ");
}
