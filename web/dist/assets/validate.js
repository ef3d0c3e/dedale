// Checks loaded JSON against the types in types.ts, so a wrong or damaged
// file fails on load with a message naming the field, not later in a view.
// Unknown fields are ignored, as Dedale's own readers do.
import { MARK_COLORS } from "./types.js";
/** Formats this app reads. */
export const REPLAY_FORMAT = 1;
export const TOURNAMENT_FORMAT = 1;
export class FormatError extends Error {
    name = "FormatError";
}
function describe(value) {
    if (value === null)
        return "null";
    if (Array.isArray(value))
        return "an array";
    if (typeof value === "string")
        return JSON.stringify(value.length > 40 ? `${value.slice(0, 40)}…` : value);
    if (typeof value === "object")
        return "an object";
    return String(value);
}
function fail(path, expected, value) {
    throw new FormatError(`${path || "the file"}: expected ${expected}, found ${describe(value)}`);
}
export const int = (value, path) => typeof value === "number" && Number.isSafeInteger(value) && value >= 0
    ? value
    : fail(path, "a whole number", value);
export const str = (value, path) => typeof value === "string" ? value : fail(path, "a string", value);
export const bool = (value, path) => typeof value === "boolean" ? value : fail(path, "true or false", value);
/** A signed integer (exit codes, signals). */
const signed = (value, path) => typeof value === "number" && Number.isSafeInteger(value)
    ? value
    : fail(path, "an integer", value);
export function nullable(check) {
    return (value, path) => (value === null || value === undefined ? null : check(value, path));
}
export function arr(check) {
    return (value, path) => {
        if (!Array.isArray(value))
            fail(path, "an array", value);
        return value.map((item, i) => check(item, `${path}[${i}]`));
    };
}
export function oneOf(...values) {
    return (value, path) => values.includes(value)
        ? value
        : fail(path, values.map((v) => `"${v}"`).join(" or "), value);
}
function field(path, key) {
    return path ? `${path}.${key}` : key;
}
export function obj(shape) {
    return (value, path) => {
        if (typeof value !== "object" || value === null || Array.isArray(value)) {
            fail(path, "an object", value);
        }
        const record = value;
        const out = {};
        for (const key of Object.keys(shape)) {
            const checked = shape[key](record[key], field(path, key));
            // Optional fields stay absent, as in the file.
            if (checked !== undefined)
                out[key] = checked;
        }
        return out;
    };
}
export const pos = (value, path) => {
    if (!Array.isArray(value) || value.length !== 2)
        fail(path, "a position [x, y]", value);
    return [int(value[0], `${path}[0]`), int(value[1], `${path}[1]`)];
};
const move = oneOf("up", "down", "left", "right", "stay");
const outcome = oneOf("ok", "illegal", "invalid", "error", "timeout", "not_ready", "crashed");
const seating = oneOf("solo", "duo", "duo_swapped");
const mode = oneOf("solo", "duo");
/** A palette colour, or `null` for anything else (as Dedale reads them). */
const markColor = (value) => MARK_COLORS.includes(value) ? value : null;
/** `path`, or absent for cells (any other kind reads as cells, as in Dedale). */
const markKind = (value) => (value === "path" ? "path" : undefined);
const mark = obj({
    cells: arr(pos),
    label: nullable(str),
    color: markColor,
    kind: markKind,
});
const botError = obj({ message: str, traceback: nullable(str) });
export const maze = (value, path) => {
    const data = obj({ width: int, height: int, cells: str })(value, path);
    if (data.width === 0 || data.height === 0 || data.cells.length !== data.width * data.height) {
        fail(field(path, "cells"), `${data.width}×${data.height} cells`, data.cells);
    }
    if (!/^[#._]*$/.test(data.cells))
        fail(field(path, "cells"), "only #, . and _", data.cells);
    return data;
};
const config = obj({ startup_ms: int, move_ms: int, max_turns: int, seating });
export const startupRecord = obj({
    outcome: oneOf("ok", "late", "init_error", "timeout", "crashed"),
    startup_us: nullable(int),
    init_us: nullable(int),
    log: arr(str),
    marks: arr(mark),
    error: nullable(botError),
});
const exitRecord = obj({
    code: nullable(signed),
    signal: nullable(signed),
    killed: bool,
    stderr_tail: str,
});
const playerRecord = obj({
    name: str,
    path: str,
    start: pos,
    startup: startupRecord,
    exit: nullable(exitRecord),
});
const playerTurn = obj({
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
export const turnRecord = obj({
    turn: int,
    players: arr(playerTurn),
    collected: arr(pos),
});
const eventRecord = (value, path) => {
    const { kind } = obj({ kind: oneOf("late", "ignored") })(value, path);
    return kind === "late"
        ? obj({
            turn: int,
            player: int,
            kind: oneOf("late"),
            for_turn: int,
        })(value, path)
        : obj({
            turn: int,
            player: int,
            kind: oneOf("ignored"),
            reason: str,
        })(value, path);
};
const gameResult = obj({
    reason: oneOf("cleared", "abandoned", "turn_limit"),
    turns: int,
    half_points: arr(int),
    winner: nullable(int),
    cleared_turn: nullable(int),
});
const summary = obj({
    count: int,
    min: int,
    mean: int,
    p50: int,
    p95: int,
    p99: int,
    max: int,
});
const playerStats = obj({
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
function checkFormat(value, expected, what) {
    const { format } = obj({ format: int })(value, "");
    if (format !== expected) {
        throw new FormatError(`unsupported ${what} format ${format} (this app reads format ${expected})`);
    }
}
/** Every player and turn agree with each other and with the maze. */
function checkConsistent(record) {
    const players = record.players.length;
    const inside = ([x, y], path) => {
        if (x >= record.maze.width || y >= record.maze.height)
            fail(path, "a position inside the maze", [x, y]);
    };
    record.players.forEach((p, i) => {
        inside(p.start, `players[${i}].start`);
    });
    record.turns.forEach((turn, t) => {
        if (turn.turn !== t + 1)
            fail(`turns[${t}].turn`, String(t + 1), turn.turn);
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
const gameRecord = obj({
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
export function parseReplay(value) {
    checkFormat(value, REPLAY_FORMAT, "replay");
    const record = gameRecord(value, "");
    checkConsistent(record);
    return record;
}
export const standing = obj({
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
const mazeSource = (value, path) => {
    if (typeof value === "object" && value !== null && "map" in value) {
        return obj({ map: obj({ path: str }) })(value, path);
    }
    return obj({
        generated: obj({ width: int, height: int, loops: int, points: int }),
    })(value, path);
};
const gameEntry = obj({
    index: int,
    code: nullable(str),
    seating,
    bots: arr(int),
    result: gameResult,
    replay: str,
});
/** A `tournament.json`. */
export function parseTournament(value) {
    checkFormat(value, TOURNAMENT_FORMAT, "tournament");
    return obj({
        format: int,
        dedale: str,
        master_seed: str,
        mode,
        maze: mazeSource,
        startup_ms: int,
        move_ms: int,
        max_turns: nullable(int),
        bots: arr(obj({ name: str, path: str })),
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
export function parseListing(value) {
    const entry = (item, path) => {
        const { kind } = obj({ kind: oneOf("game", "tournament") })(item, path);
        if (kind === "game")
            return obj({ kind: oneOf("game"), ...gameSummaryFields })(item, path);
        return obj({
            kind: oneOf("tournament"),
            path: str,
            modified: int,
            error: nullable(str),
            mode: nullable(mode),
            bots: arr(str),
            leaders: arr(str),
            games: arr(obj(gameSummaryFields)),
        })(item, path);
    };
    return obj({ entries: arr(entry), truncated: bool })(value, "");
}
/** `GET /api/session`. */
export const session = (value, path) => {
    const base = obj({
        mode: oneOf("browse", "run", "tournament"),
        title: str,
    })(value, path);
    if (base.mode === "browse")
        return base;
    return {
        ...base,
        ...obj({
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
export const pace = obj({ paused: bool, delay_ms: int });
export const gameStart = obj({
    code: nullable(str),
    maze,
    config,
    players: arr(obj({ name: str, path: str, start: pos })),
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
export const gameEnd = obj(gameEndFields);
const liveGame = obj({
    game: int,
    start: gameStart,
    startups: arr(nullable(startupRecord)),
    turns: arr(turnRecord),
    end: nullable(gameEnd),
});
const liveSnapshot = obj({
    session,
    pace,
    games: arr(liveGame),
    standings: nullable(arr(standing)),
});
/** One event of `GET /api/events`, by name. */
export function parseLiveEvent(name, value) {
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
                data: obj({ ...game, ...gameEndFields })(value, name),
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
