// Values per turn for the charts, and how to fit them on screen.
/** One value per turn (turn `i + 1` at index `i`); `null` when there was none. */
export function series(turns, player, metric) {
    return turns.map((turn) => turn.players[player]?.[metric] ?? null);
}
/** The value indices a column covers, when `n` values share `columns` columns. */
export function columnRange(column, n, columns) {
    if (n <= columns)
        return [column, column + 1];
    return [Math.floor((column * n) / columns), Math.floor(((column + 1) * n) / columns)];
}
/**
 * At most `columns` buckets holding the smallest and largest value of the
 * turns they cover, so a single spike still shows; `null` where a column
 * has no value. One bucket per value when they fit.
 */
export function downsample(values, columns) {
    const n = values.length;
    const count = Math.min(n, Math.max(1, Math.floor(columns)));
    const out = [];
    for (let column = 0; column < count; column++) {
        const [from, to] = columnRange(column, n, count);
        let bucket = null;
        for (let i = from; i < to; i++) {
            const v = values[i];
            if (v === null || v === undefined)
                continue;
            if (bucket) {
                bucket.min = Math.min(bucket.min, v);
                bucket.max = Math.max(bucket.max, v);
            }
            else
                bucket = { min: v, max: v };
        }
        out.push(bucket);
    }
    return out;
}
/** 0 to a round number at least `max`: 1, 2 or 5 times a power of ten. */
export function linearScale(max) {
    const top = Math.max(1, max);
    const power = 10 ** Math.floor(Math.log10(top));
    const round = [1, 2, 5, 10].map((m) => m * power).find((m) => m >= top) ?? top;
    const step = round / (round / power <= 2 ? 4 : 5);
    const ticks = [];
    for (let v = 0; v <= round + step / 2; v += step)
        ticks.push(Math.round(v * 1e6) / 1e6);
    return { at: (v) => Math.max(0, Math.min(1, v / round)), ticks };
}
/** 1 to a power of ten at least `max`, logarithmic: for durations. */
export function logScale(max) {
    const top = Math.max(1, Math.ceil(Math.log10(Math.max(10, max))));
    const ticks = Array.from({ length: top + 1 }, (_, k) => 10 ** k);
    return { at: (v) => Math.max(0, Math.min(1, Math.log10(Math.max(1, v)) / top)), ticks };
}
/** The turn (1 to `turns`) at `x` along `width`. */
export function turnAt(x, width, turns) {
    return Math.max(1, Math.min(turns, Math.floor((x / Math.max(1, width)) * turns) + 1));
}
/** The middle of turn `turn`'s slot along `width`. */
export function turnX(turn, width, turns) {
    return ((turn - 0.5) / Math.max(1, turns)) * width;
}
