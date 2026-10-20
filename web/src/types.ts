// The data Dedale writes and streams, as its Rust types serialize it
// (replay and tournament formats: docs/replay.md; live events: docs/web.md).

export type Pos = readonly [x: number, y: number];
export type Move = "up" | "down" | "left" | "right" | "stay";
export type Outcome = "ok" | "illegal" | "invalid" | "error" | "timeout" | "not_ready" | "crashed";
export type Seating = "solo" | "duo" | "duo_swapped";
export type EndReason = "cleared" | "abandoned" | "turn_limit";
export type StartupOutcome = "ok" | "late" | "init_error" | "timeout" | "crashed";

/** The colours a mark can take; the app chooses the shades per theme. */
export const MARK_COLORS = [
  "red",
  "orange",
  "yellow",
  "green",
  "teal",
  "blue",
  "purple",
  "pink",
] as const;
export type MarkColor = (typeof MARK_COLORS)[number];

export interface Mark {
  cells: Pos[];
  label: string | null;
  /** `null`: the player's colour. */
  color: MarkColor | null;
  /** A line through the cells in order; absent for highlighted cells. */
  kind?: "path";
}

export interface BotError {
  message: string;
  traceback: string | null;
}

export interface MazeData {
  width: number;
  height: number;
  /** Row by row: `#` wall, `.` point, `_` empty. */
  cells: string;
}

export interface ConfigRecord {
  startup_ms: number;
  move_ms: number;
  max_turns: number;
  seating: Seating;
}

export interface StartupRecord {
  outcome: StartupOutcome;
  startup_us: number | null;
  init_us: number | null;
  log: string[];
  marks: Mark[];
  error: BotError | null;
}

export interface ExitRecord {
  code: number | null;
  signal: number | null;
  killed: boolean;
  stderr_tail: string;
}

/** What a bot says about itself; `null` for what it does not say. */
export interface About {
  name: string | null;
  author: string | null;
  description: string | null;
}

export interface PlayerRecord {
  /** What the player is called: its declared name, else its file name. */
  name: string;
  path: string;
  about: About;
  start: Pos;
  startup: StartupRecord;
  exit: ExitRecord | null;
}

export interface PlayerTurnRecord {
  requested: Move | null;
  applied: Move;
  outcome: Outcome;
  latency_us: number | null;
  think_us: number | null;
  pos: Pos;
  gained: number;
  half_points: number;
  log: string[];
  marks: Mark[];
  error: BotError | null;
  invalid: string | null;
}

export interface TurnRecord {
  turn: number;
  players: PlayerTurnRecord[];
  collected: Pos[];
}

export type EventRecord =
  | { turn: number; player: number; kind: "late"; for_turn: number }
  | { turn: number; player: number; kind: "ignored"; reason: string };

export interface GameResult {
  reason: EndReason;
  turns: number;
  half_points: number[];
  winner: number | null;
  cleared_turn: number | null;
}

export interface Summary {
  count: number;
  min: number;
  mean: number;
  p50: number;
  p95: number;
  p99: number;
  max: number;
}

export interface OutcomeCounts {
  ok: number;
  illegal: number;
  invalid: number;
  error: number;
  timeout: number;
  not_ready: number;
  crashed: number;
}

export interface PlayerStats {
  startup_us: number | null;
  init_us: number | null;
  latency_us: Summary | null;
  think_us: Summary | null;
  outcomes: OutcomeCounts;
  late: number;
  turns: number;
  steps: number;
  stays: number;
  half_points: number;
  turns_to_clear: number | null;
}

/** `replay.json`. */
export interface GameRecord {
  format: number;
  dedale: string;
  code: string | null;
  maze: MazeData;
  config: ConfigRecord;
  players: PlayerRecord[];
  turns: TurnRecord[];
  events: EventRecord[];
  result: GameResult;
  stats: PlayerStats[];
}

export type Mode = "solo" | "duo";

export type MazeSource =
  | { generated: { width: number; height: number; loops: number; points: number } }
  | { map: { path: string } };

export interface Standing {
  rank: number;
  bot: number;
  name: string;
  games: number;
  match_points: number;
  wins: number;
  draws: number;
  losses: number;
  cleared: number;
  turns_to_clear: number;
  half_points: number;
  failures: number;
  latency_us: Summary | null;
}

export interface GameEntry {
  index: number;
  code: string | null;
  seating: Seating;
  bots: number[];
  result: GameResult;
  /** Relative to `tournament.json`. */
  replay: string;
}

export interface BotInfo {
  name: string;
  path: string;
  about: About;
}

/** `tournament.json`. */
export interface TournamentReport {
  format: number;
  dedale: string;
  master_seed: string;
  mode: Mode;
  maze: MazeSource;
  startup_ms: number;
  move_ms: number;
  max_turns: number | null;
  bots: BotInfo[];
  games: GameEntry[];
  leaderboard: Standing[];
}

/** A game in `GET /api/runs`. */
export interface GameSummary {
  kind: "game";
  /** Of the `replay.json`, relative to the root. */
  path: string;
  /** Seconds since the Unix epoch. */
  modified: number;
  error: string | null;
  code: string | null;
  seating: Seating | null;
  bots: string[];
  result: GameResult | null;
}

/** A tournament in `GET /api/runs`. */
export interface TournamentSummary {
  kind: "tournament";
  path: string;
  modified: number;
  error: string | null;
  mode: Mode | null;
  bots: string[];
  leaders: string[];
  games: Omit<GameSummary, "kind">[];
}

export interface Listing {
  entries: (GameSummary | TournamentSummary)[];
  truncated: boolean;
}

export type SessionMode = "browse" | "run" | "tournament";
export type Phase = "waiting" | "running" | "finished" | "failed";

/** `GET /api/session`. */
export interface Session {
  mode: SessionMode;
  title: string;
  /** Live sessions only. */
  bots?: string[];
  games?: number;
  /** Mazes and timings, in a line. */
  settings?: string;
  /** A tournament's mode. */
  tournament?: Mode | null;
  /** A run's game, before Start. */
  start?: GameStart | null;
  phase?: Phase;
  error?: string | null;
  file?: string | null;
}

export interface Pace {
  paused: boolean;
  delay_ms: number;
}

export interface PlayerStart {
  name: string;
  path: string;
  about: About;
  start: Pos;
}

export interface GameStart {
  code: string | null;
  maze: MazeData;
  config: ConfigRecord;
  players: PlayerStart[];
}

/** A finished game's record, besides its start and turns. */
export interface GameEnd {
  format: number;
  dedale: string;
  players: PlayerRecord[];
  events: EventRecord[];
  result: GameResult;
  stats: PlayerStats[];
  /** For `GET /api/file`. */
  replay: string;
}

export interface LiveGame {
  game: number;
  start: GameStart;
  startups: (StartupRecord | null)[];
  /** While it is played; a finished game's turns are in its replay. */
  turns: TurnRecord[];
  end: GameEnd | null;
}

export interface LiveSnapshot {
  session: Session;
  pace: Pace;
  games: LiveGame[];
  standings: Standing[] | null;
}

export type LiveEvent =
  | { name: "snapshot"; data: LiveSnapshot }
  | { name: "waiting"; data: Record<string, never> }
  | { name: "game_start"; data: GameStart & { game: number } }
  | { name: "startup"; data: { game: number; player: number; startup: StartupRecord } }
  | { name: "turn"; data: { game: number; turn: TurnRecord } }
  | { name: "game_end"; data: GameEnd & { game: number } }
  | { name: "standings"; data: { standings: Standing[] } }
  | { name: "pace"; data: Pace }
  | { name: "finished"; data: { file: string } }
  | { name: "error"; data: { message: string } };
