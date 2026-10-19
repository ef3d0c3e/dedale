// Draws a maze at one turn: walls (cached), points, collected cells tinted
// by who took them, players, and an overlay of marked cells.
import { cellAt, fit } from "./layout.js";
function readPalette() {
    const style = getComputedStyle(document.documentElement);
    const v = (name) => style.getPropertyValue(name).trim();
    return {
        floor: v("--floor"),
        wall: v("--wall"),
        point: v("--point"),
        players: [v("--player-a"), v("--player-b")],
        mark: v("--mark"),
        text: v("--on-player"),
    };
}
export class MazeCanvas {
    element;
    model;
    layout;
    palette = readPalette();
    walls = null;
    dpr = 1;
    constructor(model) {
        this.model = model;
        this.element = document.createElement("canvas");
        this.element.className = "maze";
        this.layout = fit(model.width, model.height, 600, 600);
    }
    /** Fits the maze in the space; returns whether the size changed. */
    resize(maxWidth, maxHeight) {
        const layout = fit(this.model.width, this.model.height, maxWidth, maxHeight);
        const dpr = window.devicePixelRatio || 1;
        if (layout.cell === this.layout.cell && dpr === this.dpr && this.walls)
            return false;
        this.layout = layout;
        this.dpr = dpr;
        this.element.width = Math.round(layout.width * dpr);
        this.element.height = Math.round(layout.height * dpr);
        this.element.style.width = `${layout.width}px`;
        this.element.style.height = `${layout.height}px`;
        this.walls = null;
        return true;
    }
    /** Colours changed (light or dark theme). */
    restyle() {
        this.palette = readPalette();
        this.walls = null;
    }
    /** The cell under a mouse event. */
    cellAt(event) {
        const box = this.element.getBoundingClientRect();
        return cellAt(this.layout, event.clientX - box.left, event.clientY - box.top);
    }
    wallLayer() {
        if (this.walls)
            return this.walls;
        const layer = document.createElement("canvas");
        layer.width = this.element.width;
        layer.height = this.element.height;
        const ctx = layer.getContext("2d");
        if (ctx) {
            ctx.scale(this.dpr, this.dpr);
            ctx.fillStyle = this.palette.floor;
            ctx.fillRect(0, 0, this.layout.width, this.layout.height);
            ctx.fillStyle = this.palette.wall;
            const c = this.layout.cell;
            for (let y = 0; y < this.model.height; y++) {
                for (let x = 0; x < this.model.width; x++) {
                    if (this.model.cell(x, y, 0) === "wall")
                        ctx.fillRect(x * c, y * c, c, c);
                }
            }
        }
        this.walls = layer;
        return layer;
    }
    draw(turn, players, overlay = null) {
        const ctx = this.element.getContext("2d");
        if (!ctx)
            return;
        const { cell: c } = this.layout;
        const p = this.palette;
        ctx.setTransform(1, 0, 0, 1, 0, 0);
        ctx.drawImage(this.wallLayer(), 0, 0);
        ctx.setTransform(this.dpr, 0, 0, this.dpr, 0, 0);
        // Cells already collected, tinted by who took them.
        ctx.globalAlpha = 0.22;
        for (const taken of this.model.taken) {
            if (taken.turn > turn)
                break;
            const colour = taken.by === 2 ? p.players[1] : p.players[0];
            ctx.fillStyle = colour ?? p.point;
            ctx.fillRect(taken.x * c, taken.y * c, c, c);
        }
        ctx.globalAlpha = 1;
        ctx.fillStyle = p.point;
        const round = c >= 6;
        const size = round ? c * 0.18 : Math.max(1, Math.floor(c / 3));
        ctx.beginPath();
        for (const [x, y] of this.model.points) {
            if ((this.model.takenOn(x, y) ?? Number.POSITIVE_INFINITY) <= turn)
                continue;
            if (round) {
                ctx.moveTo(x * c + c / 2 + size, y * c + c / 2);
                ctx.arc(x * c + c / 2, y * c + c / 2, size, 0, Math.PI * 2);
            }
            else {
                ctx.rect(x * c + (c - size) / 2, y * c + (c - size) / 2, size, size);
            }
        }
        ctx.fill();
        if (overlay)
            this.drawMarks(ctx, overlay);
        this.drawPlayers(ctx, players);
    }
    drawMarks(ctx, overlay) {
        const c = this.layout.cell;
        const own = this.palette.players[overlay.player] ?? this.palette.mark;
        for (const mark of overlay.marks) {
            ctx.fillStyle = mark.color ?? own;
            ctx.globalAlpha = 0.35;
            for (const [x, y] of mark.cells)
                ctx.fillRect(x * c, y * c, c, c);
            ctx.globalAlpha = 1;
            const first = mark.cells[0];
            if (mark.label && first && c >= 10) {
                ctx.fillStyle = this.palette.mark;
                ctx.font = `${Math.max(10, Math.round(c * 0.5))}px system-ui, sans-serif`;
                ctx.textBaseline = "bottom";
                ctx.fillText(mark.label, first[0] * c + 2, first[1] * c);
            }
        }
    }
    drawPlayers(ctx, players) {
        const c = this.layout.cell;
        const radius = Math.max(1.5, c * 0.42);
        const shared = players.length === 2 &&
            players[0]?.pos[0] === players[1]?.pos[0] &&
            players[0]?.pos[1] === players[1]?.pos[1];
        players.forEach((player, i) => {
            const [x, y] = player.pos;
            const cx = x * c + c / 2;
            const cy = y * c + c / 2;
            ctx.fillStyle = this.palette.players[i] ?? this.palette.mark;
            ctx.beginPath();
            if (shared) {
                // A on the left half, B on the right.
                const start = i === 0 ? Math.PI / 2 : -Math.PI / 2;
                ctx.moveTo(cx, cy);
                ctx.arc(cx, cy, radius, start, start + Math.PI);
            }
            else {
                ctx.arc(cx, cy, radius, 0, Math.PI * 2);
            }
            ctx.fill();
            if (c >= 14 && !shared) {
                ctx.fillStyle = this.palette.text;
                ctx.font = `bold ${Math.round(c * 0.5)}px system-ui, sans-serif`;
                ctx.textAlign = "center";
                ctx.textBaseline = "middle";
                ctx.fillText(i === 0 ? "A" : "B", cx, cy + 1);
                ctx.textAlign = "start";
            }
        });
    }
}
