// The heatmap choice under the maze, and its legend.
import { h, replace } from "../dom.js";
import { letter } from "../format.js";
import { highest, takenBy, Visits } from "./heat.js";
export class Heatmap {
    element;
    data;
    choice;
    legend;
    visits;
    constructor(data, redraw) {
        this.data = data;
        this.visits = data.players.map((_, i) => new Visits(data.model, i));
        this.choice = h("select", { "aria-label": "Heatmap", onchange: redraw }, h("option", { value: "" }, "no heatmap"), data.players.map((p, i) => h("option", { value: `visits:${i}` }, `${letter(i)} ${p.name}: cells visited`)), h("option", { value: "order" }, "points: order taken"));
        this.legend = h("span", { class: "legend muted" });
        this.element = h("div", { class: "heatmap" }, this.choice, this.legend);
    }
    /** The layer to draw at `turn`, with its legend updated. */
    layer(turn) {
        const value = this.choice.value;
        if (value.startsWith("visits:")) {
            const player = Number(value.slice("visits:".length));
            const visits = this.visits[player];
            if (!visits)
                return null;
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
    reset() {
        this.visits = this.data.players.map((_, i) => new Visits(this.data.model, i));
    }
    showLegend(low, high, tone) {
        replace(this.legend, low, h("span", { class: `ramp ramp-${tone}` }), high);
    }
}
