// The game screen: header, maze, timeline (scrubber, controls, problem
// markers, keyboard) and the stats table. Works for recorded games and,
// through `update`, for games being played.
import { aboutList } from "../about.js";
import { h, pick, replace } from "../dom.js";
import { describeResult, formatSpread, formatUs, letter, outcomeName, SPREAD_HELP, score, } from "../format.js";
import { MazeCanvas } from "./canvas.js";
import { advance, DEFAULT_SPEED, SPEEDS, step } from "./playback.js";
import { markers, nextProblem, previousProblem, problemStarts, } from "./timeline.js";
const MARKER_ROW = 7;
/** Half the turn slider's thumb (16 px in Chromium and Firefox): its centre
 * runs from this far in to this far before the end, and so do the markers. */
const THUMB_HALF = 8;
const KIND_COLOURS = {
    illegal: "--kind-illegal",
    invalid: "--kind-invalid",
    error: "--kind-error",
    timeout: "--kind-timeout",
    not_ready: "--kind-not-ready",
    crashed: "--kind-crashed",
    late: "--kind-late",
};
function button(label, title, onClick) {
    return h("button", { type: "button", title, "aria-label": title, onclick: onClick }, label);
}
/** Whether a key press belongs to a text field rather than to the page. */
function typing(target) {
    if (target instanceof HTMLInputElement)
        return target.type !== "range";
    return target instanceof HTMLSelectElement || target instanceof HTMLTextAreaElement;
}
export class GameView {
    element;
    /** Where the turn inspector goes. */
    side;
    data;
    options;
    canvas;
    board;
    head;
    hover;
    /** The maze and what goes under it (the heatmap legend). */
    boardColumn;
    range;
    turnLabel;
    playButton;
    strip;
    speedChoice;
    stats;
    turn = 0;
    speed = DEFAULT_SPEED;
    frame = null;
    lastTime = 0;
    carry = 0;
    found = [];
    starts = [];
    cleanups = [];
    constructor(data, options = {}) {
        this.data = data;
        this.options = options;
        this.canvas = new MazeCanvas(data.model);
        this.head = h("div", { class: "game-head" });
        this.hover = h("div", { class: "hover muted" }, " ");
        this.board = h("div", { class: "board" }, this.canvas.element);
        this.boardColumn = h("div", { class: "board-column" }, this.board, this.hover);
        this.side = h("aside", { class: "side" });
        this.range = h("input", {
            type: "range",
            min: 0,
            max: 0,
            value: 0,
            "aria-label": "Turn",
            oninput: () => {
                this.pause();
                this.setTurn(Number(this.range.value));
            },
        });
        this.turnLabel = h("span", { class: "turn-label" });
        this.playButton = button("▶", "Play (space)", () => this.toggle());
        const speed = h("select", {
            "aria-label": "Speed",
            title: "Turns per second",
            onchange: () => {
                this.speed = Number(speed.value);
            },
        }, SPEEDS.map((s) => h("option", { value: s, selected: s === DEFAULT_SPEED }, `${s}/s`)));
        this.speedChoice = speed;
        this.strip = h("canvas", {
            class: "markers",
            height: MARKER_ROW * 2,
            title: "Problems, a row per player: click one to go to its turn",
        });
        this.strip.addEventListener("click", (event) => {
            const box = this.strip.getBoundingClientRect();
            const span = Math.max(1, box.width - 2 * THUMB_HALF);
            const share = (event.clientX - box.left - THUMB_HALF) / span;
            this.pause();
            this.setTurn(Math.round(share * this.last));
        });
        this.stats = h("div", { class: "stats" });
        const controls = h("div", { class: "controls" }, button("⏮", "First turn (Home)", () => this.jump(0)), button("←", "Previous turn (←)", () => this.jump(step(this.turn, -1, this.last))), this.playButton, button("→", "Next turn (→)", () => this.jump(step(this.turn, 1, this.last))), button("⏭", "Last turn (End)", () => this.jump(this.last)), button("⟨!", "Previous problem (p)", () => this.problem(-1)), button("!⟩", "Next problem (n)", () => this.problem(1)), speed, this.turnLabel);
        this.element = h("section", { class: "game" }, this.head, aboutList(data.players, true), h("div", { class: "game-body" }, this.boardColumn, this.side), h("div", { class: "timeline" }, controls, this.range, this.strip), this.stats);
        this.canvas.element.addEventListener("mousemove", (event) => this.describeCell(event));
        this.canvas.element.addEventListener("mouseleave", () => {
            this.hover.textContent = " ";
        });
        const onKey = (event) => this.key(event);
        document.addEventListener("keydown", onKey);
        this.cleanups.push(() => document.removeEventListener("keydown", onKey));
        const resize = new ResizeObserver(() => this.fit());
        resize.observe(this.board);
        this.cleanups.push(() => resize.disconnect());
        const dark = matchMedia("(prefers-color-scheme: dark)");
        const restyle = () => {
            this.canvas.restyle();
            this.drawStrip();
            this.draw();
        };
        dark.addEventListener("change", restyle);
        this.cleanups.push(() => dark.removeEventListener("change", restyle));
        this.update();
        this.setTurn(options.turn ?? 0);
    }
    /** The last turn played. */
    get last() {
        return this.data.model.length;
    }
    get current() {
        return this.turn;
    }
    /** Playback speed, in turns per second. */
    get playbackSpeed() {
        return this.speed;
    }
    /** A speed the menu does not offer is ignored. */
    set playbackSpeed(speed) {
        if (pick(this.speedChoice, String(speed)))
            this.speed = speed;
    }
    /** The game gained turns, or ended. */
    update() {
        this.found = markers(this.data.model.turns, this.data.events);
        this.starts = problemStarts(this.found);
        this.range.max = String(this.last);
        this.drawStrip();
        this.renderStats();
        this.renderHead();
    }
    /** Shows `turn` (clamped to the turns played). */
    setTurn(turn, playing = false) {
        this.turn = step(0, turn, this.last);
        this.range.value = String(this.turn);
        this.turnLabel.textContent = `turn ${this.turn} / ${this.last}`;
        this.renderHead();
        this.draw();
        this.options.onShow?.(this.turn);
        if (!playing)
            this.options.onTurn?.(this.turn);
    }
    /** Redraws the maze (after the overlay changed). */
    draw() {
        const overlays = this.options.overlays?.(this.turn) ?? [];
        const heat = this.options.heat?.(this.turn) ?? null;
        this.canvas.draw(this.turn, this.data.model.state(this.turn).players, overlays, heat);
    }
    dispose() {
        this.pause();
        for (const cleanup of this.cleanups)
            cleanup();
    }
    /** Stops playback and shows `turn`. */
    goTo(turn) {
        this.pause();
        this.setTurn(turn);
    }
    jump(turn) {
        this.goTo(turn);
    }
    problem(direction) {
        const target = direction > 0 ? nextProblem(this.starts, this.turn) : previousProblem(this.starts, this.turn);
        if (target !== null)
            this.jump(target);
    }
    toggle() {
        if (this.frame === null)
            this.play();
        else
            this.pause();
    }
    play() {
        if (this.frame !== null)
            return;
        if (this.turn >= this.last)
            this.setTurn(0, true);
        this.playButton.textContent = "⏸";
        this.playButton.title = "Pause (space)";
        this.carry = 0;
        this.lastTime = performance.now();
        const tick = (now) => {
            const next = advance(this.turn, this.carry, now - this.lastTime, this.speed, this.last);
            this.lastTime = now;
            this.carry = next.carry;
            if (next.turn !== this.turn)
                this.setTurn(next.turn, true);
            if (next.done)
                this.pause();
            else
                this.frame = requestAnimationFrame(tick);
        };
        this.frame = requestAnimationFrame(tick);
    }
    pause() {
        if (this.frame === null)
            return;
        cancelAnimationFrame(this.frame);
        this.frame = null;
        this.playButton.textContent = "▶";
        this.playButton.title = "Play (space)";
        this.options.onTurn?.(this.turn);
    }
    key(event) {
        if (typing(event.target) || event.ctrlKey || event.metaKey || event.altKey)
            return;
        switch (event.key) {
            case "ArrowLeft":
                this.jump(step(this.turn, -1, this.last));
                break;
            case "ArrowRight":
                this.jump(step(this.turn, 1, this.last));
                break;
            case " ":
                this.toggle();
                break;
            case "Home":
                this.jump(0);
                break;
            case "End":
                this.jump(this.last);
                break;
            case "n":
                this.problem(1);
                break;
            case "p":
                this.problem(-1);
                break;
            default:
                return;
        }
        event.preventDefault();
    }
    fit() {
        const maxHeight = Math.max(160, Math.round(window.innerHeight * 0.7));
        if (this.canvas.resize(this.board.clientWidth, maxHeight))
            this.draw();
        this.drawStrip();
    }
    describeCell(event) {
        const cell = this.canvas.cellAt(event);
        if (!cell)
            return;
        const [x, y] = cell;
        const what = this.data.model.cell(x, y, this.turn);
        const taken = this.data.model.takenOn(x, y);
        const note = taken !== null && taken <= this.turn ? `, point taken on turn ${taken}` : "";
        this.hover.textContent = `(${x}, ${y}) ${what}${note}`;
    }
    renderHead() {
        const { model, players, config, end } = this.data;
        const state = model.state(this.turn);
        const scores = players.map((p, i) => h("span", { class: `chip player-${i}` }, h("b", {}, letter(i)), ` ${p.name} `, h("b", {}, score(state.players[i]?.half_points ?? 0))));
        replace(this.head, h("div", { class: "head-line" }, h("span", {}, `turn ${state.turn} / ${config.max_turns}`), h("span", {}, `${state.remaining} points left`), scores, h("span", { class: "muted" }, this.data.code ?? "map")), end
            ? h("div", { class: "result" }, describeResult(end.result, config.seating, players.map((p) => p.name)))
            : null);
    }
    /** The problems under the slider, each where the thumb shows its turn; hidden without any. */
    drawStrip() {
        this.strip.hidden = this.found.length === 0;
        if (this.strip.hidden)
            return;
        const width = this.range.clientWidth || 600;
        const dpr = window.devicePixelRatio || 1;
        const rows = Math.max(1, this.data.players.length);
        this.strip.width = Math.round(width * dpr);
        this.strip.height = Math.round(rows * MARKER_ROW * dpr);
        this.strip.style.width = `${width}px`;
        this.strip.style.height = `${rows * MARKER_ROW}px`;
        const ctx = this.strip.getContext("2d");
        if (!ctx)
            return;
        ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
        ctx.clearRect(0, 0, width, rows * MARKER_ROW);
        const style = getComputedStyle(document.documentElement);
        const last = Math.max(1, this.last);
        const span = Math.max(1, width - 2 * THUMB_HALF);
        const tick = Math.max(1, span / last);
        for (const marker of this.found) {
            ctx.fillStyle = style.getPropertyValue(KIND_COLOURS[marker.kind]).trim();
            const x = THUMB_HALF + ((marker.turn - 0.5) / last) * span;
            ctx.fillRect(x, marker.player * MARKER_ROW + 1, tick, MARKER_ROW - 2);
        }
    }
    renderStats() {
        const end = this.data.end;
        if (!end) {
            replace(this.stats, h("p", { class: "muted" }, "Statistics come when the game is over."));
            return;
        }
        const problems = (i) => {
            const stats = end.stats[i];
            if (!stats)
                return "-";
            const counts = Object.entries(stats.outcomes)
                .filter(([outcome, n]) => outcome !== "ok" && n > 0)
                .map(([outcome, n]) => `${n} ${outcomeName(outcome)}`);
            if (stats.late > 0)
                counts.push(`${stats.late} late`);
            return counts.join(", ") || "-";
        };
        const rows = this.data.players.map((p, i) => {
            const s = end.stats[i];
            return h("tr", {}, h("td", {}, h("span", { class: `chip player-${i}` }, h("b", {}, letter(i)), ` ${p.name}`)), h("td", { class: "num" }, score(end.result.half_points[i] ?? 0)), h("td", { class: "num" }, s?.steps ?? "-"), h("td", { class: "num" }, s?.stays ?? "-"), h("td", { class: "num" }, formatUs(s?.startup_us)), h("td", { class: "num" }, formatUs(s?.init_us)), h("td", { class: "num" }, formatSpread(s?.latency_us)), h("td", { class: "num" }, formatSpread(s?.think_us)), h("td", {}, problems(i)));
        });
        replace(this.stats, h("table", {}, h("thead", {}, h("tr", {}, [
            "player",
            "score",
            "steps",
            "stays",
            "startup",
            "init",
            "latency avg / 5% / 1%",
            "move() avg / 5% / 1%",
            "problems",
        ].map((t) => h("th", t.endsWith("avg / 5% / 1%") ? { title: SPREAD_HELP } : {}, t)))), h("tbody", {}, rows)));
    }
}
