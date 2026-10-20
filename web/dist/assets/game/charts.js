// Latency, time in move() and score over the whole game, drawn with uPlot
// (vendored, MIT): the values under the cursor in each legend, cursors and
// zoom shared by every chart (drag to zoom, double-click to see it all), a
// scanner at the turn shown, and a click to show a turn.
import { h } from "../dom.js";
import { formatUs, letter, outcomeName, score } from "../format.js";
import uPlot from "../vendor/uplot/uPlot.esm.js";
import { durationTick, forLog, points, pointsLeft, powersOfTen, problems, series, } from "./series.js";
const HEIGHT = 150;
/** A press that moves less than this is a click, not a zoom. */
const CLICK_PX = 4;
function palette() {
    const style = getComputedStyle(document.documentElement);
    const v = (name) => style.getPropertyValue(name).trim();
    return {
        a: v("--player-a"),
        b: v("--player-b"),
        muted: v("--muted"),
        line: v("--line"),
        budget: v("--kind-timeout"),
        problem: v("--kind-error"),
    };
}
const TITLES = {
    latency: "latency",
    think: "time in move()",
    score: "score",
};
/** Charts are grouped per screen, so screens never share a cursor. */
let screens = 0;
export class Charts {
    element;
    data;
    onJump;
    body;
    key = `dedale-charts-${++screens}`;
    cleanups = [];
    plots = [];
    turn = 0;
    syncing = false;
    zoomed = false;
    constructor(data, onJump) {
        this.data = data;
        this.onJump = onJump;
        this.body = h("div", { class: "chart-list" });
        this.element = h("details", { class: "charts", open: true }, h("summary", {}, "Charts"), this.body);
        const resize = new ResizeObserver(() => this.resize());
        resize.observe(this.body);
        this.cleanups.push(() => resize.disconnect());
        // Colours come from the theme: build again when it changes.
        const dark = matchMedia("(prefers-color-scheme: dark)");
        const rebuild = () => this.build();
        dark.addEventListener("change", rebuild);
        this.cleanups.push(() => dark.removeEventListener("change", rebuild));
        this.build();
    }
    /** The game gained turns or ended. */
    update() {
        for (const p of this.plots)
            p.plot.setData(this.rows(p.kind), !this.zoomed);
        this.placeAll();
    }
    /** Moves the scanner to the shown turn. */
    mark(turn) {
        this.turn = turn;
        this.placeAll();
    }
    dispose() {
        for (const p of this.plots)
            p.plot.destroy();
        this.plots = [];
        for (const cleanup of this.cleanups)
            cleanup();
    }
    get turns() {
        return this.data.model.turns;
    }
    build() {
        for (const p of this.plots)
            p.plot.destroy();
        this.body.replaceChildren();
        const colours = palette();
        this.plots = ["latency", "think", "score"].map((kind) => {
            const target = h("div", { class: "chart" });
            this.body.append(h("div", { class: "chart-block" }, h("div", { class: "chart-title" }, TITLES[kind]), target));
            const plot = new uPlot(this.options(kind, colours), this.rows(kind), target);
            const scanner = h("div", { class: "scanner", hidden: true });
            if (kind === "latency")
                scanner.append(h("span", { class: "scanner-turn" }));
            plot.over.append(scanner);
            const p = { kind, plot, scanner, hovered: false };
            this.clicks(p);
            return p;
        });
        this.resize();
    }
    /** The data of one chart: the turns, then one row per series. */
    rows(kind) {
        const turns = this.turns;
        const x = turns.map((_, i) => i + 1);
        const players = this.data.players.map((_, i) => i);
        const budget = this.data.config.move_ms * 1000;
        switch (kind) {
            case "latency":
                return [
                    x,
                    ...players.map((i) => forLog(series(turns, i, "latency_us"))),
                    // A problem sits on its latency, or on the budget when there was none.
                    ...players.map((i) => problems(turns, i, (t) => Math.max(1, turns[t]?.players[i]?.latency_us ?? budget))),
                ];
            case "think":
                return [x, ...players.map((i) => forLog(series(turns, i, "think_us")))];
            case "score":
                return [
                    x,
                    ...players.map((i) => points(series(turns, i, "half_points"))),
                    pointsLeft(this.data.model.state(0).remaining, turns),
                ];
        }
    }
    options(kind, colours) {
        const log = kind !== "score";
        const stepped = kind === "score" ? uPlot.paths.stepped?.({ align: 1 }) : undefined;
        const format = log ? formatUs : (v) => score(Math.round(v * 2));
        const shown = (v) => v === null || v === undefined ? "-" : format(v);
        const budget = this.data.config.move_ms * 1000;
        const players = this.data.players.map((p, i) => ({
            label: `${letter(i)} ${p.name}`,
            stroke: i === 0 ? colours.a : colours.b,
            width: 1.5,
            value: (_u, v) => shown(v),
            ...(stepped ? { paths: stepped } : {}),
        }));
        const extra = kind === "latency"
            ? this.data.players.map((_, i) => ({
                label: `${letter(i)} problems`,
                stroke: colours.problem,
                fill: colours.problem,
                paths: () => null,
                points: { show: true, size: 7, stroke: colours.problem, fill: colours.problem },
                value: (_u, v, _s, index) => {
                    const outcome = index === null ? undefined : this.turns[index]?.players[i]?.outcome;
                    return v === null || outcome === undefined ? "-" : outcomeName(outcome);
                },
            }))
            : kind === "score"
                ? [
                    {
                        label: "points left",
                        stroke: colours.muted,
                        width: 1,
                        dash: [4, 4],
                        value: (_u, v) => shown(v),
                    },
                ]
                : [];
        const axis = {
            stroke: colours.muted,
            grid: { stroke: colours.line, width: 1 },
            ticks: { stroke: colours.line, width: 1 },
            font: "11px system-ui, sans-serif",
        };
        return {
            width: 600,
            height: HEIGHT,
            series: [
                { label: "turn", value: (_u, v) => (v === null ? "-" : String(v)) },
                ...players,
                ...extra,
            ],
            scales: {
                x: {
                    time: false,
                    // Every turn when nothing is zoomed, at least two so an empty game draws.
                    range: (_u, min, max) => [
                        Number.isFinite(min) ? min : 1,
                        Number.isFinite(max) && max > min ? max : Math.max(2, this.turns.length),
                    ],
                },
                y: log
                    ? {
                        distr: 3,
                        log: 10,
                        range: (_u, _min, max) => uPlot.rangeLog(1, Math.max(Number.isFinite(max) ? max : 1, budget), 10, true),
                    }
                    : { range: (_u, _min, max) => [0, Math.max(1, Number.isFinite(max) ? max : 1)] },
            },
            axes: [
                axis,
                {
                    ...axis,
                    size: 64,
                    values: (_u, ticks) => ticks.map((t) => (t === null ? "" : log ? durationTick(t) : format(t))),
                    ...(log
                        ? {
                            splits: (_u, _axis, min, max) => powersOfTen(min, max),
                            // Every power of ten keeps its label.
                            filter: (_u, splits) => splits,
                        }
                        : {}),
                },
            ],
            cursor: {
                sync: { key: this.key },
                drag: { x: true, y: false },
                points: { size: 6 },
            },
            hooks: {
                draw: log ? [(u) => this.drawBudget(u, budget, colours.budget)] : [],
                setScale: [(u, key) => key === "x" && this.scaled(u)],
                setSize: [() => this.placeAll()],
            },
        };
    }
    /** The move budget across a duration chart, dashed. */
    drawBudget(u, budget, colour) {
        const y = Math.round(u.valToPos(budget, "y", true));
        const { left, top, width, height } = u.bbox;
        if (y < top || y > top + height)
            return;
        const ctx = u.ctx;
        const dpr = window.devicePixelRatio || 1;
        ctx.save();
        ctx.strokeStyle = colour;
        ctx.lineWidth = 1.5 * dpr;
        ctx.setLineDash([6 * dpr, 4 * dpr]);
        ctx.beginPath();
        ctx.moveTo(left, y);
        ctx.lineTo(left + width, y);
        ctx.stroke();
        ctx.restore();
    }
    /** One chart zoomed: the others follow, and the scanner moves. */
    scaled(source) {
        const { min, max } = source.scales["x"] ?? {};
        if (min === undefined || max === undefined)
            return;
        this.zoomed = min > 1 || max < this.turns.length;
        if (!this.syncing) {
            this.syncing = true;
            for (const p of this.plots) {
                if (p.plot !== source)
                    p.plot.setScale("x", { min, max });
            }
            this.syncing = false;
        }
        this.placeAll();
    }
    /** A click (a press that hardly moves) shows the turn under it. */
    clicks(p) {
        const plot = p.plot;
        let down = null;
        plot.over.addEventListener("mouseenter", () => {
            p.hovered = true;
        });
        plot.over.addEventListener("mouseleave", () => {
            p.hovered = false;
            // After uPlot clears the legend for the pointer leaving.
            setTimeout(() => this.place(p));
        });
        plot.over.addEventListener("mousedown", (event) => {
            down = event.clientX;
        });
        plot.over.addEventListener("mouseup", (event) => {
            const n = this.turns.length;
            if (down !== null && Math.abs(event.clientX - down) < CLICK_PX && n > 0) {
                const turn = Math.round(plot.posToVal(event.offsetX, "x"));
                this.onJump(Math.max(1, Math.min(n, turn)));
            }
            down = null;
        });
    }
    resize() {
        const width = this.body.clientWidth;
        if (width === 0)
            return;
        for (const p of this.plots)
            p.plot.setSize({ width, height: HEIGHT });
    }
    placeAll() {
        for (const p of this.plots)
            this.place(p);
    }
    place(p) {
        const { min, max } = p.plot.scales["x"] ?? {};
        const turn = this.turn;
        const visible = turn >= 1 && min !== undefined && max !== undefined && turn >= min && turn <= max;
        p.scanner.hidden = !visible;
        // Away from the pointer, the legend reads the turn shown.
        if (!p.hovered && turn >= 1 && turn <= this.turns.length) {
            p.plot.setLegend({ idx: turn - 1 }, false);
        }
        if (!visible)
            return;
        p.scanner.style.left = `${p.plot.valToPos(turn, "x")}px`;
        const label = p.scanner.firstElementChild;
        if (label)
            label.textContent = `turn ${turn}`;
    }
}
