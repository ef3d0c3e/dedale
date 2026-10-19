// Latency, time in move() and score over the whole game, drawn on canvas:
// hover reads the values, a click shows that turn.
import { h } from "../dom.js";
import { formatUs, letter, score } from "../format.js";
import { downsample, linearScale, logScale, series, turnAt, turnX, } from "./series.js";
import { markers } from "./timeline.js";
const HEIGHT = 110;
const GUTTER = 64;
const MARKS = 6;
function colours() {
    const style = getComputedStyle(document.documentElement);
    const v = (name) => style.getPropertyValue(name).trim();
    return {
        a: v("--player-a"),
        b: v("--player-b"),
        line: v("--line"),
        muted: v("--muted"),
        fg: v("--fg"),
        timeout: v("--kind-timeout"),
        problem: v("--kind-error"),
    };
}
export class Charts {
    element;
    data;
    charts;
    readout;
    turn = 0;
    constructor(data, onJump) {
        this.data = data;
        const chart = (title, metric, format) => ({
            title,
            metric,
            canvas: h("canvas", { class: "chart" }),
            cache: document.createElement("canvas"),
            format,
        });
        this.charts = [
            chart("latency", "latency_us", formatUs),
            chart("in move()", "think_us", formatUs),
            chart("score", "half_points", score),
        ];
        this.readout = h("div", { class: "readout muted" }, " ");
        this.element = h("details", { class: "charts", open: true }, h("summary", {}, "Charts"), this.charts.map((c) => h("div", { class: "chart-block" }, h("div", { class: "chart-title" }, c.title), c.canvas)), this.readout);
        for (const c of this.charts) {
            c.canvas.addEventListener("mousemove", (event) => this.hover(c, event));
            c.canvas.addEventListener("mouseleave", () => {
                this.readout.textContent = " ";
            });
            c.canvas.addEventListener("click", (event) => {
                const turn = this.turnUnder(c, event);
                if (turn !== null)
                    onJump(turn);
            });
        }
        new ResizeObserver(() => this.update()).observe(this.element);
    }
    /** Redraws everything (new turns, resize, theme). */
    update() {
        for (const c of this.charts)
            this.render(c);
        this.mark(this.turn);
    }
    /** Marks the shown turn. */
    mark(turn) {
        this.turn = turn;
        const n = this.data.model.length;
        const fg = colours().fg;
        for (const c of this.charts) {
            const ctx = c.canvas.getContext("2d");
            if (!ctx || c.cache.width === 0)
                continue;
            ctx.setTransform(1, 0, 0, 1, 0, 0);
            ctx.clearRect(0, 0, c.canvas.width, c.canvas.height);
            ctx.drawImage(c.cache, 0, 0);
            if (turn > 0 && n > 0) {
                const dpr = window.devicePixelRatio || 1;
                const plot = c.canvas.width / dpr - GUTTER;
                ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
                ctx.fillStyle = fg;
                ctx.fillRect(GUTTER + turnX(turn, plot, n) - 0.5, 0, 1, HEIGHT);
            }
        }
    }
    turnUnder(c, event) {
        const n = this.data.model.length;
        const box = c.canvas.getBoundingClientRect();
        const x = event.clientX - box.left - GUTTER;
        return n > 0 && x >= 0 ? turnAt(x, box.width - GUTTER, n) : null;
    }
    hover(c, event) {
        const turn = this.turnUnder(c, event);
        if (turn === null)
            return;
        const record = this.data.model.turns[turn - 1];
        const parts = this.data.players.map((p, i) => {
            const value = record?.players[i]?.[c.metric];
            return `${letter(i)} ${p.name} ${value === null || value === undefined ? "-" : c.format(value)}`;
        });
        this.readout.textContent = `turn ${turn} · ${c.title}: ${parts.join(" · ")}`;
    }
    render(c) {
        const dpr = window.devicePixelRatio || 1;
        const width = Math.max(GUTTER + 50, c.canvas.parentElement?.clientWidth ?? 600);
        const plot = width - GUTTER;
        c.canvas.width = Math.round(width * dpr);
        c.canvas.height = Math.round(HEIGHT * dpr);
        c.canvas.style.width = `${width}px`;
        c.canvas.style.height = `${HEIGHT}px`;
        c.cache.width = c.canvas.width;
        c.cache.height = c.canvas.height;
        const ctx = c.cache.getContext("2d");
        if (!ctx)
            return;
        ctx.scale(dpr, dpr);
        const palette = colours();
        const turns = this.data.model.turns;
        const n = turns.length;
        const values = this.data.players.map((_, i) => series(turns, i, c.metric));
        const highest = Math.max(0, ...values.flat().map((v) => v ?? 0));
        const timeoutUs = this.data.config.move_ms * 1000;
        const scale = c.metric === "half_points"
            ? linearScale(highest / 2)
            : logScale(Math.max(highest, timeoutUs));
        const value = (v) => (c.metric === "half_points" ? v / 2 : v);
        const plotTop = 8;
        const plotHeight = HEIGHT - plotTop - MARKS - 2;
        const y = (v) => plotTop + plotHeight * (1 - scale.at(value(v)));
        ctx.font = "11px system-ui, sans-serif";
        ctx.textBaseline = "middle";
        ctx.textAlign = "right";
        for (const tick of scale.ticks) {
            const ty = plotTop + plotHeight * (1 - scale.at(tick));
            ctx.fillStyle = palette.line;
            ctx.fillRect(GUTTER, Math.round(ty), plot, 1);
            ctx.fillStyle = palette.muted;
            ctx.fillText(c.metric === "half_points" ? String(tick) : formatUs(tick), GUTTER - 6, ty);
        }
        if (c.metric !== "half_points") {
            // The move budget.
            ctx.fillStyle = palette.timeout;
            ctx.fillRect(GUTTER, Math.round(y(timeoutUs)), plot, 1.5);
        }
        if (n === 0)
            return;
        values.forEach((points, i) => {
            ctx.fillStyle = i === 0 ? palette.a : palette.b;
            const buckets = downsample(points, plot);
            const step = plot / buckets.length;
            buckets.forEach((bucket, column) => {
                if (!bucket)
                    return;
                const top = y(bucket.max);
                const bottom = y(bucket.min);
                const x = GUTTER + column * step;
                ctx.fillRect(x, top - 1, Math.max(1, step), Math.max(2, bottom - top + 2));
            });
        });
        if (c.metric === "latency_us") {
            // Problems, as ticks under the plot.
            ctx.fillStyle = palette.problem;
            for (const m of markers(turns, this.data.events)) {
                const x = GUTTER + turnX(m.turn, plot, n);
                ctx.fillRect(x - 0.5, HEIGHT - MARKS + (m.player === 0 ? 0 : MARKS / 2), Math.max(1, plot / n), MARKS / 2);
            }
        }
    }
}
