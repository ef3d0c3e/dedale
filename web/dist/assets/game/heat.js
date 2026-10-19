// Heatmaps over the maze: how often each player stood on each cell, and
// the order points were taken in.
/**
 * How many states (turn 0 and after each turn) found a player on each
 * cell, up to a turn. Kept incrementally: scrubbing by a turn costs one
 * cell update, whatever the game's length.
 */
export class Visits {
    counts;
    model;
    player;
    /** The last state counted; -1 before any. */
    upTo = -1;
    constructor(model, player) {
        this.model = model;
        this.player = player;
        this.counts = new Uint32Array(model.width * model.height);
    }
    index(state) {
        const [x, y] = state === 0
            ? (this.model.starts[this.player] ?? [0, 0])
            : (this.model.turns[state - 1]?.players[this.player]?.pos ?? [0, 0]);
        return y * this.model.width + x;
    }
    /** The counts over states 0 to `turn` (clamped to the turns played). */
    at(turn) {
        const target = Math.max(0, Math.min(turn, this.model.length));
        while (this.upTo < target) {
            this.upTo++;
            const i = this.index(this.upTo);
            this.counts[i] = (this.counts[i] ?? 0) + 1;
        }
        while (this.upTo > target) {
            const i = this.index(this.upTo);
            this.counts[i] = (this.counts[i] ?? 1) - 1;
            this.upTo--;
        }
        return this.counts;
    }
}
/** The largest count. */
export function highest(counts) {
    let max = 0;
    for (const c of counts)
        if (c > max)
            max = c;
    return max;
}
/** The points taken up to `turn`, in order. */
export function takenBy(model, turn) {
    const out = [];
    for (const taken of model.taken) {
        if (taken.turn > turn)
            break;
        out.push(taken);
    }
    return out;
}
/** How dark a cell is drawn, 0.15 for the faintest to 0.85: grows with `value / max`. */
export function strength(value, max) {
    return max <= 0 ? 0 : 0.15 + 0.7 * Math.min(1, value / max);
}
