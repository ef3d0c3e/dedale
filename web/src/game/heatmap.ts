// The heatmap choice under the maze, and its legend.

import { h, pick, replace } from "../dom.ts";
import { letter } from "../format.ts";
import type { GameData } from "./data.ts";
import { type HeatLayer, highest, takenBy, Visits } from "./heat.ts";

export class Heatmap {
  readonly element: HTMLElement;
  private readonly data: GameData;
  private readonly choice: HTMLSelectElement;
  private readonly legend: HTMLElement;
  private visits: Visits[];

  constructor(data: GameData, redraw: () => void) {
    this.data = data;
    this.visits = data.players.map((_, i) => new Visits(data.model, i));
    this.choice = h(
      "select",
      { "aria-label": "Heatmap", onchange: redraw },
      h("option", { value: "" }, "no heatmap"),
      data.players.map((p, i) =>
        h("option", { value: `visits:${i}` }, `${letter(i)} ${p.name}: cells visited`),
      ),
      h("option", { value: "order" }, "points: order taken"),
    );
    this.legend = h("span", { class: "legend muted" });
    this.element = h("div", { class: "heatmap" }, this.choice, this.legend);
  }

  /** The heatmap shown, as the menu's value. */
  get selected(): string {
    return this.choice.value;
  }

  /** A choice this game does not offer is ignored. */
  set selected(value: string) {
    pick(this.choice, value);
  }

  /** The layer to draw at `turn`, with its legend updated. */
  layer(turn: number): HeatLayer | null {
    const value = this.choice.value;
    if (value.startsWith("visits:")) {
      const player = Number(value.slice("visits:".length));
      const visits = this.visits[player];
      if (!visits) return null;
      const counts = visits.at(turn);
      const max = highest(counts);
      this.showLegend("1", `${max} ${max === 1 ? "visit" : "visits"}`, `player-${player}`);
      return { kind: "visits", player, counts, max };
    }
    if (value === "order") {
      const taken = takenBy(this.data.model, turn);
      const first = taken[0]?.turn ?? 1;
      const last = taken.at(-1)?.turn ?? first;
      this.showLegend(`turn ${first}`, `turn ${last}`, "order");
      return { kind: "order", taken, first, last };
    }
    replace(this.legend);
    return null;
  }

  /** A live game's model was rebuilt: start counting again. */
  reset(): void {
    this.visits = this.data.players.map((_, i) => new Visits(this.data.model, i));
  }

  private showLegend(low: string, high: string, tone: string): void {
    replace(this.legend, low, h("span", { class: `ramp ramp-${tone}` }), high);
  }
}
