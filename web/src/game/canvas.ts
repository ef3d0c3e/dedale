// Draws a maze at one turn: walls (cached), points, collected cells tinted
// by who took them, players, and an overlay of marked cells.

import type { GameModel, PlayerState } from "../model.ts";
import type { Mark } from "../types.ts";
import { type HeatLayer, strength } from "./heat.ts";
import { cellAt, fit, type Layout } from "./layout.ts";

interface Palette {
  floor: string;
  wall: string;
  point: string;
  players: string[];
  shared: string;
  mark: string;
  text: string;
}

function readPalette(): Palette {
  const style = getComputedStyle(document.documentElement);
  const v = (name: string): string => style.getPropertyValue(name).trim();
  return {
    floor: v("--floor"),
    wall: v("--wall"),
    point: v("--point"),
    players: [v("--player-a"), v("--player-b")],
    shared: v("--player-both"),
    mark: v("--mark"),
    text: v("--on-player"),
  };
}

/** A bot-chosen colour, if it is a plain colour value (`#rgb`, `#rrggbb`,
 * `#rrggbbaa` or a name). */
export function markColour(colour: string | null): string | null {
  return colour !== null &&
    /^(#([0-9a-fA-F]{3,4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|[a-zA-Z]{1,30})$/.test(colour)
    ? colour
    : null;
}

/** Marks drawn over the maze, in a player's colour unless they set theirs. */
export interface Overlay {
  player: number;
  marks: readonly Mark[];
}

export class MazeCanvas {
  readonly element: HTMLCanvasElement;
  private readonly model: GameModel;
  private layout: Layout;
  private palette = readPalette();
  private walls: HTMLCanvasElement | null = null;
  private dpr = 1;

  constructor(model: GameModel) {
    this.model = model;
    this.element = document.createElement("canvas");
    this.element.className = "maze";
    this.layout = fit(model.width, model.height, 600, 600);
  }

  /** Fits the maze in the space; returns whether the size changed. */
  resize(maxWidth: number, maxHeight: number): boolean {
    const layout = fit(this.model.width, this.model.height, maxWidth, maxHeight);
    const dpr = window.devicePixelRatio || 1;
    const width = Math.round(layout.width * dpr);
    const height = Math.round(layout.height * dpr);
    // The canvas itself, not the last layout: it starts at the default size.
    if (width === this.element.width && height === this.element.height && dpr === this.dpr)
      return false;
    this.layout = layout;
    this.dpr = dpr;
    this.element.width = width;
    this.element.height = height;
    this.element.style.width = `${layout.width}px`;
    this.element.style.height = `${layout.height}px`;
    this.walls = null;
    return true;
  }

  /** Colours changed (light or dark theme). */
  restyle(): void {
    this.palette = readPalette();
    this.walls = null;
  }

  /** The cell under a mouse event. */
  cellAt(event: MouseEvent): [number, number] | null {
    const box = this.element.getBoundingClientRect();
    return cellAt(this.layout, event.clientX - box.left, event.clientY - box.top);
  }

  private wallLayer(): HTMLCanvasElement {
    if (this.walls) return this.walls;
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
          if (this.model.cell(x, y, 0) === "wall") ctx.fillRect(x * c, y * c, c, c);
        }
      }
    }
    this.walls = layer;
    return layer;
  }

  draw(
    turn: number,
    players: readonly PlayerState[],
    overlay: Overlay | null = null,
    heat: HeatLayer | null = null,
  ): void {
    const ctx = this.element.getContext("2d");
    if (!ctx) return;
    const { cell: c } = this.layout;
    const p = this.palette;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.drawImage(this.wallLayer(), 0, 0);
    ctx.setTransform(this.dpr, 0, 0, this.dpr, 0, 0);

    if (heat) {
      this.drawHeat(ctx, heat);
    } else {
      // Cells already collected, tinted by who took them.
      ctx.globalAlpha = 0.22;
      for (const taken of this.model.taken) {
        if (taken.turn > turn) break;
        ctx.fillStyle = this.takerColour(taken.by);
        ctx.fillRect(taken.x * c, taken.y * c, c, c);
      }
      ctx.globalAlpha = 1;
    }

    ctx.fillStyle = p.point;
    const round = c >= 6;
    const size = round ? c * 0.18 : Math.max(1, Math.floor(c / 3));
    ctx.beginPath();
    for (const [x, y] of this.model.points) {
      if ((this.model.takenOn(x, y) ?? Number.POSITIVE_INFINITY) <= turn) continue;
      if (round) {
        ctx.moveTo(x * c + c / 2 + size, y * c + c / 2);
        ctx.arc(x * c + c / 2, y * c + c / 2, size, 0, Math.PI * 2);
      } else {
        ctx.rect(x * c + (c - size) / 2, y * c + (c - size) / 2, size, size);
      }
    }
    ctx.fill();

    if (overlay) this.drawMarks(ctx, overlay);
    this.drawPlayers(ctx, players);
  }

  /** A's colour, B's, or the mark colour for a point both took. */
  takerColour(by: number): string {
    if (by === 1) return this.palette.players[0] ?? this.palette.point;
    if (by === 2) return this.palette.players[1] ?? this.palette.point;
    return this.palette.shared;
  }

  private drawHeat(ctx: CanvasRenderingContext2D, heat: HeatLayer): void {
    const c = this.layout.cell;
    if (heat.kind === "visits") {
      ctx.fillStyle = this.palette.players[heat.player] ?? this.palette.mark;
      const width = this.model.width;
      heat.counts.forEach((count, i) => {
        if (count === 0) return;
        ctx.globalAlpha = strength(count, heat.max);
        ctx.fillRect((i % width) * c, Math.floor(i / width) * c, c, c);
      });
    } else {
      const span = Math.max(1, heat.last - heat.first);
      for (const taken of heat.taken) {
        ctx.fillStyle = this.takerColour(taken.by);
        ctx.globalAlpha = strength(taken.turn - heat.first + 1, span + 1);
        ctx.fillRect(taken.x * c, taken.y * c, c, c);
      }
    }
    ctx.globalAlpha = 1;
  }

  private drawMarks(ctx: CanvasRenderingContext2D, overlay: Overlay): void {
    const c = this.layout.cell;
    const own = this.palette.players[overlay.player] ?? this.palette.mark;
    for (const mark of overlay.marks) {
      ctx.fillStyle = markColour(mark.color) ?? own;
      ctx.globalAlpha = 0.35;
      for (const [x, y] of mark.cells) ctx.fillRect(x * c, y * c, c, c);
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

  private drawPlayers(ctx: CanvasRenderingContext2D, players: readonly PlayerState[]): void {
    const c = this.layout.cell;
    const radius = Math.max(1.5, c * 0.42);
    const shared =
      players.length === 2 &&
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
      } else {
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
