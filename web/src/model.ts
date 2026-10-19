// A game's state at any turn, rebuilt from the stored positions and
// collections without simulating anything.
//
// Instead of checkpoints, each cell remembers the turn its point was taken
// (`takenAt`): the board after turn t is a lookup per cell, and positions
// and scores are stored per turn, so any turn of a 255×255 game is
// immediate. A game being played grows turn by turn with `push`.

import type { GameRecord, MazeData, Move, Pos, TurnRecord } from "./types.ts";

export type Cell = "wall" | "empty" | "point";

/** Never collected. */
const NEVER = 0x7fffffff;

export interface PlayerState {
  pos: Pos;
  half_points: number;
  /** The move applied on the last turn; `null` before the first. */
  last_move: Move | null;
}

/** A point taken: where, on which turn, by whom (bit 0 for A, bit 1 for B). */
export interface Taken {
  x: number;
  y: number;
  turn: number;
  by: number;
}

export interface GameState {
  /** Turns played. */
  turn: number;
  players: PlayerState[];
  /** Points left on the board. */
  remaining: number;
}

export class GameModel {
  readonly width: number;
  readonly height: number;
  readonly starts: readonly Pos[];
  readonly turns: TurnRecord[] = [];
  /** The initial maze, one character per cell. */
  private readonly cells: string;
  private readonly takenAt: Int32Array;
  /** Points left after each turn, from turn 0. */
  private readonly remainingAfter: number[];
  /** Every point taken, in order. */
  readonly taken: Taken[] = [];
  /** The cells holding a point at the start. */
  readonly points: readonly (readonly [number, number])[];

  constructor(maze: MazeData, starts: readonly Pos[], turns: readonly TurnRecord[] = []) {
    this.width = maze.width;
    this.height = maze.height;
    this.cells = maze.cells;
    this.starts = starts;
    this.takenAt = new Int32Array(maze.width * maze.height).fill(NEVER);
    const points: [number, number][] = [];
    for (let i = 0; i < maze.cells.length; i++) {
      if (maze.cells[i] === ".") points.push([i % maze.width, Math.floor(i / maze.width)]);
    }
    this.points = points;
    this.remainingAfter = [points.length];
    for (const turn of turns) this.push(turn);
  }

  static fromRecord(record: GameRecord): GameModel {
    return new GameModel(
      record.maze,
      record.players.map((p) => p.start),
      record.turns,
    );
  }

  /** Turns played so far. */
  get length(): number {
    return this.turns.length;
  }

  /** Adds the next turn of a game being played. */
  push(turn: TurnRecord): void {
    const t = this.turns.length + 1;
    let remaining = this.remainingAfter[t - 1] ?? 0;
    for (const [x, y] of turn.collected) {
      const i = this.index(x, y);
      if (i === null || (this.takenAt[i] ?? NEVER) !== NEVER) continue;
      this.takenAt[i] = t;
      if (this.cells[i] === ".") {
        remaining--;
        let by = 0;
        turn.players.forEach((p, player) => {
          if (p.pos[0] === x && p.pos[1] === y) by |= 1 << player;
        });
        this.taken.push({ x, y, turn: t, by });
      }
    }
    this.turns.push(turn);
    this.remainingAfter.push(remaining);
  }

  private index(x: number, y: number): number | null {
    return x < this.width && y < this.height ? y * this.width + x : null;
  }

  /** The cell at `(x, y)` after `turn` turns. */
  cell(x: number, y: number, turn: number): Cell {
    const i = this.index(x, y);
    if (i === null) return "wall";
    if ((this.takenAt[i] ?? NEVER) <= turn) return "empty";
    switch (this.cells[i]) {
      case "#":
        return "wall";
      case ".":
        return "point";
      default:
        return "empty";
    }
  }

  /** The turn a point at `(x, y)` was collected, if it was. */
  takenOn(x: number, y: number): number | null {
    const i = this.index(x, y);
    const at = i === null ? NEVER : (this.takenAt[i] ?? NEVER);
    return at === NEVER ? null : at;
  }

  /** The board after `turn` turns, in the text format (`#`, `.`, `_`). */
  board(turn: number): string {
    let out = "";
    for (let y = 0; y < this.height; y++) {
      for (let x = 0; x < this.width; x++) {
        const cell = this.cell(x, y, turn);
        out += cell === "wall" ? "#" : cell === "point" ? "." : "_";
      }
    }
    return out;
  }

  /** The state after `turn` turns, clamped to the turns played. */
  state(turn: number): GameState {
    const t = Math.max(0, Math.min(turn, this.turns.length));
    const last = this.turns[t - 1];
    const players = last
      ? last.players.map((p) => ({ pos: p.pos, half_points: p.half_points, last_move: p.applied }))
      : this.starts.map((pos) => ({ pos, half_points: 0, last_move: null }));
    return { turn: t, players, remaining: this.remainingAfter[t] ?? 0 };
  }
}
