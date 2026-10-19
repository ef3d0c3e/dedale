// A game's state at any turn, rebuilt from the stored positions and
// collections without simulating anything.
//
// Instead of checkpoints, each cell remembers the turn its point was taken
// (`takenAt`): the board after turn t is a lookup per cell, and positions
// and scores are stored per turn, so any turn of a 255×255 game is
// immediate. A game being played grows turn by turn with `push`.
/** Never collected. */
const NEVER = 0x7fffffff;
export class GameModel {
    width;
    height;
    starts;
    turns = [];
    /** The initial maze, one character per cell. */
    cells;
    takenAt;
    /** Points left after each turn, from turn 0. */
    remainingAfter;
    constructor(maze, starts, turns = []) {
        this.width = maze.width;
        this.height = maze.height;
        this.cells = maze.cells;
        this.starts = starts;
        this.takenAt = new Int32Array(maze.width * maze.height).fill(NEVER);
        let points = 0;
        for (const c of maze.cells)
            if (c === ".")
                points++;
        this.remainingAfter = [points];
        for (const turn of turns)
            this.push(turn);
    }
    static fromRecord(record) {
        return new GameModel(record.maze, record.players.map((p) => p.start), record.turns);
    }
    /** Turns played so far. */
    get length() {
        return this.turns.length;
    }
    /** Adds the next turn of a game being played. */
    push(turn) {
        const t = this.turns.length + 1;
        let remaining = this.remainingAfter[t - 1] ?? 0;
        for (const [x, y] of turn.collected) {
            const i = this.index(x, y);
            if (i === null || (this.takenAt[i] ?? NEVER) !== NEVER)
                continue;
            this.takenAt[i] = t;
            if (this.cells[i] === ".")
                remaining--;
        }
        this.turns.push(turn);
        this.remainingAfter.push(remaining);
    }
    index(x, y) {
        return x < this.width && y < this.height ? y * this.width + x : null;
    }
    /** The cell at `(x, y)` after `turn` turns. */
    cell(x, y, turn) {
        const i = this.index(x, y);
        if (i === null)
            return "wall";
        if ((this.takenAt[i] ?? NEVER) <= turn)
            return "empty";
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
    takenOn(x, y) {
        const i = this.index(x, y);
        const at = i === null ? NEVER : (this.takenAt[i] ?? NEVER);
        return at === NEVER ? null : at;
    }
    /** The board after `turn` turns, in the text format (`#`, `.`, `_`). */
    board(turn) {
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
    state(turn) {
        const t = Math.max(0, Math.min(turn, this.turns.length));
        const last = this.turns[t - 1];
        const players = last
            ? last.players.map((p) => ({ pos: p.pos, half_points: p.half_points, last_move: p.applied }))
            : this.starts.map((pos) => ({ pos, half_points: 0, last_move: null }));
        return { turn: t, players, remaining: this.remainingAfter[t] ?? 0 };
    }
}
