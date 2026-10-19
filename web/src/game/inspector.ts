// The turn inspector, beside the maze: one panel per player, and whose
// marks are drawn on the maze.

import { h, replace } from "../dom.ts";
import { markColour, type Overlay } from "./canvas.ts";
import type { GameData } from "./data.ts";
import { inspect, type PlayerPanel } from "./inspect.ts";

export class Inspector {
  readonly element: HTMLElement;
  private readonly data: GameData;
  private readonly body: HTMLElement;
  private readonly choice: HTMLSelectElement;
  private panels: PlayerPanel[] = [];
  /** Whose marks are drawn; `null` for nobody. */
  private marksOf: number | null = 0;

  constructor(data: GameData, redraw: () => void) {
    this.data = data;
    this.choice = h(
      "select",
      {
        "aria-label": "Marks drawn on the maze",
        onchange: () => {
          this.marksOf = this.choice.value === "" ? null : Number(this.choice.value);
          redraw();
        },
      },
      data.players.map((p, i) =>
        h("option", { value: i }, `${i === 0 ? "A" : "B"} ${p.name}'s marks`),
      ),
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

  /** What to draw over the maze at the shown turn. */
  overlay(): Overlay | null {
    if (this.marksOf === null) return null;
    const panel = this.panels[this.marksOf];
    return panel ? { player: this.marksOf, marks: panel.marks } : null;
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
              panel.marks.map((m) =>
                h(
                  "li",
                  {},
                  swatch(m.color),
                  `${m.label ?? "unlabelled"}: ${m.cells.length} ${m.cells.length === 1 ? "cell" : "cells"}`,
                ),
              ),
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

/** A square of a mark's colour (through the style object: the page's
 * policy forbids style attributes). */
function swatch(colour: string | null): HTMLElement | null {
  const valid = markColour(colour);
  if (!valid) return null;
  const square = h("span", { class: "swatch" });
  square.style.background = valid;
  return square;
}
