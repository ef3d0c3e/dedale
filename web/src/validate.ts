// Checks loaded JSON against the types in types.ts, so a wrong or damaged
// file fails on load with a message naming the field, not later in a view.
// Unknown fields are ignored, as Dedale's own readers do.

import type {
  About,
  BotError,
  ConfigRecord,
  EventRecord,
  ExitRecord,
  GameEnd,
  GameEntry,
  GameRecord,
  GameResult,
  GameStart,
  GameSummary,
  Listing,
  LiveEvent,
  LiveGame,
  LiveSnapshot,
  Mark,
  MarkColor,
  MazeData,
  MazeSource,
  Pace,
  PlayerRecord,
  PlayerStats,
  PlayerTurnRecord,
  Pos,
  Session,
  Standing,
  StartupRecord,
  Summary,
  TournamentReport,
  TournamentSummary,
  TurnRecord,
} from "./types.ts";
import { MARK_COLORS } from "./types.ts";

/** Formats this app reads. */
export const REPLAY_FORMAT = 1;
export const TOURNAMENT_FORMAT = 1;

export class FormatError extends Error {
  override name = "FormatError";
}

/** Checks a value found at `path` and returns it typed. */
export type Check<T> = (value: unknown, path: string) => T;

function describe(value: unknown): string {
  if (value === null) return "null";
  if (Array.isArray(value)) return "an array";
  if (typeof value === "string")
    return JSON.stringify(value.length > 40 ? `${value.slice(0, 40)}…` : value);
  if (typeof value === "object") return "an object";
  return String(value);
}

function fail(path: string, expected: string, value: unknown): never {
  throw new FormatError(`${path || "the file"}: expected ${expected}, found ${describe(value)}`);
}

export const int: Check<number> = (value, path) =>
  typeof value === "number" && Number.isSafeInteger(value) && value >= 0
    ? value
    : fail(path, "a whole number", value);

export const str: Check<string> = (value, path) =>
  typeof value === "string" ? value : fail(path, "a string", value);

export const bool: Check<boolean> = (value, path) =>
  typeof value === "boolean" ? value : fail(path, "true or false", value);

/** A signed integer (exit codes, signals). */
const signed: Check<number> = (value, path) =>
  typeof value === "number" && Number.isSafeInteger(value)
    ? value
    : fail(path, "an integer", value);

export function nullable<T>(check: Check<T>): Check<T | null> {
  return (value, path) => (value === null || value === undefined ? null : check(value, path));
}

export function arr<T>(check: Check<T>): Check<T[]> {
  return (value, path) => {
    if (!Array.isArray(value)) fail(path, "an array", value);
    return value.map((item, i) => check(item, `${path}[${i}]`));
  };
}

export function oneOf<const T extends string>(...values: T[]): Check<T> {
  return (value, path) =>
    values.includes(value as T)
      ? (value as T)
      : fail(path, values.map((v) => `"${v}"`).join(" or "), value);
}

function field(path: string, key: string): string {
  return path ? `${path}.${key}` : key;
}

export function obj<T>(shape: { [K in keyof T]-?: Check<T[K]> }): Check<T> {
  return (value, path) => {
    if (typeof value !== "object" || value === null || Array.isArray(value)) {
      fail(path, "an object", value);
    }
    const record = value as Record<string, unknown>;
    const out: Record<string, unknown> = {};
    for (const key of Object.keys(shape) as (keyof T & string)[]) {
      const checked = shape[key](record[key], field(path, key));
      // Optional fields stay absent, as in the file.
      if (checked !== undefined) out[key] = checked;
    }
    return out as T;
  };
}

export const pos: Check<Pos> = (value, path) => {
  if (!Array.isArray(value) || value.length !== 2) fail(path, "a position [x, y]", value);
  return [int(value[0], `${path}[0]`), int(value[1], `${path}[1]`)];
};

const move = oneOf("up", "down", "left", "right", "stay");
const outcome = oneOf("ok", "illegal", "invalid", "error", "timeout", "not_ready", "crashed");
const seating = oneOf("solo", "duo", "duo_swapped");
const mode = oneOf("solo", "duo");

/** A palette colour, or `null` for anything else (as Dedale reads them). */
const markColor: Check<MarkColor | null> = (value) =>
  MARK_COLORS.includes(value as MarkColor) ? (value as MarkColor) : null;

/** `path`, or absent for cells (any other kind reads as cells, as in Dedale). */
const markKind: Check<"path" | undefined> = (value) => (value === "path" ? "path" : undefined);

const mark: Check<Mark> = obj<Mark>({
  cells: arr(pos),
  label: nullable(str),
  color: markColor,
  kind: markKind,
});
const botError: Check<BotError> = obj<BotError>({ message: str, traceback: nullable(str) });

export const maze: Check<MazeData> = (value, path) => {
  const data = obj<MazeData>({ width: int, height: int, cells: str })(value, path);
  if (data.width === 0 || data.height === 0 || data.cells.length !== data.width * data.height) {
    fail(field(path, "cells"), `${data.width}×${data.height} cells`, data.cells);
  }
  if (!/^[#._]*$/.test(data.cells)) fail(field(path, "cells"), "only #, . and _", data.cells);
  return data;
};

const config = obj<ConfigRecord>({ startup_ms: int, move_ms: int, max_turns: int, seating });

export const startupRecord: Check<StartupRecord> = obj<StartupRecord>({
  outcome: oneOf("ok", "late", "init_error", "timeout", "crashed"),
  startup_us: nullable(int),
  init_us: nullable(int),
  log: arr(str),
  marks: arr(mark),
  error: nullable(botError),
});

const exitRecord = obj<ExitRecord>({
  code: nullable(signed),
  signal: nullable(signed),
  killed: bool,
  stderr_tail: str,
});

/** Older files say nothing about their bots. */
const about: Check<About> = (value, path) =>
  obj<About>({ name: nullable(str), author: nullable(str), description: nullable(str) })(
    value ?? {},
    path,
  );

const playerRecord = obj<PlayerRecord>({
  name: str,
  path: str,
  about,
  start: pos,
  startup: startupRecord,
  exit: nullable(exitRecord),
});

const playerTurn = obj<PlayerTurnRecord>({
  requested: nullable(move),
  applied: move,
  outcome,
  latency_us: nullable(int),
  think_us: nullable(int),
  pos,
  gained: int,
  half_points: int,
  log: arr(str),
  marks: arr(mark),
  error: nullable(botError),
  invalid: nullable(str),
});

export const turnRecord: Check<TurnRecord> = obj<TurnRecord>({
  turn: int,
  players: arr(playerTurn),
  collected: arr(pos),
});

const eventRecord: Check<EventRecord> = (value, path) => {
  const { kind } = obj<{ kind: "late" | "ignored" }>({ kind: oneOf("late", "ignored") })(
    value,
    path,
  );
  return kind === "late"
    ? obj<EventRecord & { kind: "late" }>({
        turn: int,
        player: int,
        kind: oneOf("late"),
        for_turn: int,
      })(value, path)
    : obj<EventRecord & { kind: "ignored" }>({
        turn: int,
        player: int,
        kind: oneOf("ignored"),
        reason: str,
      })(value, path);
};

const gameResult = obj<GameResult>({
  reason: oneOf("cleared", "abandoned", "turn_limit"),
  turns: int,
  half_points: arr(int),
  winner: nullable(int),
  cleared_turn: nullable(int),
});

const summary = obj<Summary>({
  count: int,
  min: int,
  mean: int,
  p50: int,
  p95: int,
  p99: int,
  max: int,
});

const playerStats = obj<PlayerStats>({
  startup_us: nullable(int),
  init_us: nullable(int),
  latency_us: nullable(summary),
  think_us: nullable(summary),
  outcomes: obj({
    ok: int,
    illegal: int,
    invalid: int,
    error: int,
    timeout: int,
    not_ready: int,
    crashed: int,
  }),
  late: int,
  turns: int,
  steps: int,
  stays: int,
  half_points: int,
  turns_to_clear: nullable(int),
});

/** The file's `format`, which must be `expected`. */
function checkFormat(value: unknown, expected: number, what: string): void {
  const { format } = obj<{ format: number }>({ format: int })(value, "");
  if (format !== expected) {
    throw new FormatError(
      `unsupported ${what} format ${format} (this app reads format ${expected})`,
    );
  }
}

/** Every player and turn agree with each other and with the maze. */
function checkConsistent(record: GameRecord): void {
  const players = record.players.length;
  const inside = ([x, y]: Pos, path: string): void => {
    if (x >= record.maze.width || y >= record.maze.height)
      fail(path, "a position inside the maze", [x, y]);
  };
  record.players.forEach((p, i) => {
    inside(p.start, `players[${i}].start`);
  });
  record.turns.forEach((turn, t) => {
    if (turn.turn !== t + 1) fail(`turns[${t}].turn`, String(t + 1), turn.turn);
    if (turn.players.length !== players)
      fail(`turns[${t}].players`, `${players} players`, turn.players);
    turn.players.forEach((p, i) => {
      inside(p.pos, `turns[${t}].players[${i}].pos`);
    });
    turn.collected.forEach((c, i) => {
      inside(c, `turns[${t}].collected[${i}]`);
    });
  });
  if (record.result.half_points.length !== players) {
    fail("result.half_points", `${players} scores`, record.result.half_points);
  }
}

const gameRecord = obj<GameRecord>({
  format: int,
  dedale: str,
  code: nullable(str),
  maze,
  config,
  players: arr(playerRecord),
  turns: arr(turnRecord),
  events: arr(eventRecord),
  result: gameResult,
  stats: arr(playerStats),
});

/** A `replay.json`. */
export function parseReplay(value: unknown): GameRecord {
  checkFormat(value, REPLAY_FORMAT, "replay");
  const record = gameRecord(value, "");
  checkConsistent(record);
  return record;
}

export const standing = obj<Standing>({
  rank: int,
  bot: int,
  name: str,
  games: int,
  match_points: int,
  wins: int,
  draws: int,
  losses: int,
  cleared: int,
  turns_to_clear: int,
  half_points: int,
  failures: int,
  latency_us: nullable(summary),
});

const mazeSource: Check<MazeSource> = (value, path) => {
  if (typeof value === "object" && value !== null && "map" in value) {
    return obj<{ map: { path: string } }>({ map: obj({ path: str }) })(value, path);
  }
  return obj<{ generated: { width: number; height: number; loops: number; points: number } }>({
    generated: obj({ width: int, height: int, loops: int, points: int }),
  })(value, path);
};

const gameEntry = obj<GameEntry>({
  index: int,
  code: nullable(str),
  seating,
  bots: arr(int),
  result: gameResult,
  replay: str,
});

/** A `tournament.json`. */
export function parseTournament(value: unknown): TournamentReport {
  checkFormat(value, TOURNAMENT_FORMAT, "tournament");
  return obj<TournamentReport>({
    format: int,
    dedale: str,
    master_seed: str,
    mode,
    maze: mazeSource,
    startup_ms: int,
    move_ms: int,
    max_turns: nullable(int),
    bots: arr(obj({ name: str, path: str, about })),
    games: arr(gameEntry),
    leaderboard: arr(standing),
  })(value, "");
}

const gameSummaryFields = {
  path: str,
  modified: int,
  error: nullable(str),
  code: nullable(str),
  seating: nullable(seating),
  bots: arr(str),
  result: nullable(gameResult),
};

/** `GET /api/runs`. */
export function parseListing(value: unknown): Listing {
  const entry = (item: unknown, path: string): GameSummary | TournamentSummary => {
    const { kind } = obj<{ kind: "game" | "tournament" }>({ kind: oneOf("game", "tournament") })(
      item,
      path,
    );
    if (kind === "game")
      return obj<GameSummary>({ kind: oneOf("game"), ...gameSummaryFields })(item, path);
    return obj<TournamentSummary>({
      kind: oneOf("tournament"),
      path: str,
      modified: int,
      error: nullable(str),
      mode: nullable(mode),
      bots: arr(str),
      leaders: arr(str),
      games: arr(obj<Omit<GameSummary, "kind">>(gameSummaryFields)),
    })(item, path);
  };
  return obj<Listing>({ entries: arr(entry), truncated: bool })(value, "");
}

/** `GET /api/session`. */
export const session: Check<Session> = (value, path) => {
  const base = obj<{ mode: Session["mode"]; title: string }>({
    mode: oneOf("browse", "run", "tournament"),
    title: str,
  })(value, path);
  if (base.mode === "browse") return base;
  return {
    ...base,
    ...obj<Required<Omit<Session, "mode" | "title">>>({
      bots: arr(str),
      games: int,
      settings: str,
      tournament: nullable(mode),
      start: nullable((v, p) => gameStart(v, p)),
      phase: oneOf("waiting", "running", "finished", "failed"),
      error: nullable(str),
      file: nullable(str),
    })(value, path),
  };
};

export const pace = obj<Pace>({ paused: bool, delay_ms: int });

export const gameStart = obj<GameStart>({
  code: nullable(str),
  maze,
  config,
  players: arr(obj({ name: str, path: str, about, start: pos })),
});

const gameEndFields = {
  format: int,
  dedale: str,
  players: arr(playerRecord),
  events: arr(eventRecord),
  result: gameResult,
  stats: arr(playerStats),
  replay: str,
};

export const gameEnd = obj<GameEnd>(gameEndFields);

const liveGame = obj<LiveGame>({
  game: int,
  start: gameStart,
  startups: arr(nullable(startupRecord)),
  turns: arr(turnRecord),
  end: nullable(gameEnd),
});

const liveSnapshot = obj<LiveSnapshot>({
  session,
  pace,
  games: arr(liveGame),
  standings: nullable(arr(standing)),
});

/** One event of `GET /api/events`, by name. */
export function parseLiveEvent(name: string, value: unknown): LiveEvent {
  const game = { game: int };
  switch (name) {
    case "snapshot":
      return { name, data: liveSnapshot(value, name) };
    case "waiting":
      return { name, data: {} };
    case "game_start":
      return { name, data: { ...gameStart(value, name), ...obj(game)(value, name) } };
    case "startup":
      return { name, data: obj({ ...game, player: int, startup: startupRecord })(value, name) };
    case "turn":
      return { name, data: obj({ ...game, turn: turnRecord })(value, name) };
    case "game_end":
      return {
        name,
        data: obj<GameEnd & { game: number }>({ ...game, ...gameEndFields })(value, name),
      };
    case "standings":
      return { name, data: obj({ standings: arr(standing) })(value, name) };
    case "pace":
      return { name, data: pace(value, name) };
    case "finished":
      return { name, data: obj({ file: str })(value, name) };
    case "error":
      return { name, data: obj({ message: str })(value, name) };
    default:
      throw new FormatError(`unknown event ${JSON.stringify(name)}`);
  }
}
