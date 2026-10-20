// Values per turn for the charts, their spread, and how to fit them on screen.
/** One value per turn (turn `i + 1` at index `i`); `null` when there was none. */
export function series(turns, player, metric) {
    return turns.map((turn) => turn.players[player]?.[metric] ?? null);
}
/** Durations for a logarithmic axis, which has no 0: less than 1 µs reads as 1 µs. */
export function forLog(values) {
    return values.map((v) => (v === null ? null : Math.max(1, v)));
}
/** Points (not half-points) per turn. */
export function points(values) {
    return values.map((v) => (v === null ? null : v / 2));
}
/**
 * Where to draw `player`'s problems: at each turn whose outcome was not
 * `ok`, `at(index)`; `null` on the other turns.
 */
export function problems(turns, player, at) {
    return turns.map((turn, i) => {
        const outcome = turn.players[player]?.outcome;
        return outcome === undefined || outcome === "ok" ? null : at(i);
    });
}
/** Points still in the maze after each turn, from `total` at the start. */
export function pointsLeft(total, turns) {
    let left = total;
    return turns.map((turn) => {
        left -= turn.collected.length;
        return left;
    });
}
/**
 * The spread of the values that are there, computed as in the game's
 * statistics (nearest-rank percentiles); `undefined` without any.
 */
export function spread(values) {
    const sorted = values.filter((v) => v !== null).sort((a, b) => a - b);
    const n = sorted.length;
    if (n === 0)
        return undefined;
    const sum = sorted.reduce((total, v) => total + v, 0);
    // Rank ceil(p·n/100), 1-based.
    const rank = (p) => sorted[Math.max(1, Math.ceil((p * n) / 100)) - 1] ?? 0;
    return { mean: Math.floor(sum / n), p95: rank(95), p99: rank(99) };
}
/** Powers of ten from `min` to `max`: the lines of a logarithmic axis. */
export function powersOfTen(min, max) {
    const out = [];
    for (let v = 10 ** Math.ceil(Math.log10(Math.max(1, min))); v <= max; v *= 10)
        out.push(v);
    return out;
}
/** A power of ten of microseconds as an axis label: `10 µs`, `1 ms`, `1 s`. */
export function durationTick(us) {
    if (us < 1000)
        return `${us} µs`;
    if (us < 1_000_000)
        return `${us / 1000} ms`;
    return `${us / 1_000_000} s`;
}
