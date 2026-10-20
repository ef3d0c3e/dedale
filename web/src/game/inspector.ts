// The turn inspector, beside the maze: one panel per player, and whose
// marks are drawn on the maze (one player's, both, or nobody's).

import { h, pick, replace } from "../dom.ts";
import type { Mark } from "../types.ts";
import type { Overlay } from "./canvas.ts";
import type { GameData } from "./data.ts";
import { inspect, type MarksOf, overlays, type PlayerPanel } from "./inspect.ts";

export class Inspector {
  readonly element: HTMLElement;
  private readonly data: GameData;
  private readonly body: HTMLElement;
  private readonly choice: HTMLSelectElement;
  private panels: PlayerPanel[] = [];
  private marksOf: MarksOf = 0;

  constructor(data: GameData, redraw: () => void) {
    this.data = data;
    this.choice = h(
      "select",
      {
        "aria-label": "Marks drawn on the maze",
        onchange: () => {
          this.marksOf = marksOf(this.choice.value);
          redraw();
        },
      },
      data.players.map((p, i) =>
        h("option", { value: i }, `${i === 0 ? "A" : "B"} ${p.name}'s marks`),
      ),
      data.players.length > 1 ? h("option", { value: "both" }, "both players' marks") : null,
      h("option", { value: "" }, "no marks"),
    );
    this.body = h("div", { class: "panels" });
    this.element = h(
      "div",
      { class: "inspector" },
      h("div", { class: "inspector-head" }, this.choice),
      this.body,
    );
  }

  /** Whose marks are drawn, as the menu's value (`0`, `1`, `both`, or `` for none). */
  get marks(): string {
    return this.choice.value;
  }

  /** A choice this game does not offer (B's marks in a solo game) is ignored. */
  set marks(value: string) {
    if (pick(this.choice, value)) this.marksOf = marksOf(value);
  }

  /** What to draw over the maze at the shown turn. */
  overlays(): Overlay[] {
    return overlays(this.panels, this.marksOf);
  }

  show(turn: number): void {
    this.panels = inspect(this.data, turn);
    replace(
      this.body,
      this.panels.map((panel) => this.panel(panel, turn)),
    );
  }

  private panel(panel: PlayerPanel, turn: number): HTMLElement {
    return h(
      "section",
      { class: `panel player-${panel.player}` },
      h(
        "h3",
        {},
        h(
          "span",
          { class: `chip player-${panel.player}` },
          h("b", {}, panel.title.slice(0, 1)),
          panel.title.slice(1),
        ),
        h("span", { class: "muted" }, turn === 0 ? " startup" : ` turn ${turn}`),
      ),
      h(
        "dl",
        {},
        panel.fields.map((f) => [
          h("dt", {}, f.label),
          h("dd", { class: `tone-${f.tone}` }, f.value),
        ]),
      ),
      panel.invalid ? h("p", { class: "tone-bad" }, `unreadable answer: ${panel.invalid}`) : null,
      panel.error
        ? h(
            "div",
            { class: "bot-error" },
            h("p", { class: "tone-bad" }, panel.error.message),
            panel.error.traceback ? h("pre", { class: "scroll" }, panel.error.traceback) : null,
          )
        : null,
      panel.events.length
        ? h(
            "ul",
            { class: "events" },
            panel.events.map((e) => h("li", {}, e)),
          )
        : null,
      panel.log.length
        ? [h("h4", {}, "log"), h("pre", { class: "scroll log" }, panel.log.join("\n"))]
        : null,
      panel.marks.length
        ? [
            h("h4", {}, "marks"),
            h(
              "ul",
              { class: "marks" },
              panel.marks.map((m) => h("li", {}, swatch(m, panel.player), describeMark(m))),
            ),
          ]
        : null,
      panel.exit
        ? [
            h("h4", {}, "exit"),
            h("p", {}, panel.exit.summary),
            panel.exit.stderr ? h("pre", { class: "scroll log" }, panel.exit.stderr) : null,
          ]
        : null,
    );
  }
}

function marksOf(value: string): MarksOf {
  return value === "" ? null : value === "both" ? "both" : Number(value);
}

/** `route: a path through 12 cells`, `targets: 3 cells`. */
export function describeMark(mark: Mark): string {
  const n = mark.cells.length;
  const cells = `${n} ${n === 1 ? "cell" : "cells"}`;
  return `${mark.label ?? "unlabelled"}: ${mark.kind === "path" ? `a path through ${cells}` : cells}`;
}

/** A square of a mark's colour, or a short line for a path (styled through
 * the style object: the page's policy forbids style attributes). */
function swatch(mark: Mark, player: number): HTMLElement {
  const sample = h("span", { class: mark.kind === "path" ? "swatch line" : "swatch" });
  sample.style.background = mark.color
    ? `var(--mark-${mark.color})`
    : `var(--player-${player === 0 ? "a" : "b"})`;
  return sample;
}
