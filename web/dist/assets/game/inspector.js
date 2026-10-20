// The turn inspector, beside the maze: one panel per player, and whose
// marks are drawn on the maze.
import { h, replace } from "../dom.js";
import { inspect } from "./inspect.js";
export class Inspector {
    element;
    data;
    body;
    choice;
    panels = [];
    /** Whose marks are drawn; `null` for nobody. */
    marksOf = 0;
    constructor(data, redraw) {
        this.data = data;
        this.choice = h("select", {
            "aria-label": "Marks drawn on the maze",
            onchange: () => {
                this.marksOf = this.choice.value === "" ? null : Number(this.choice.value);
                redraw();
            },
        }, data.players.map((p, i) => h("option", { value: i }, `${i === 0 ? "A" : "B"} ${p.name}'s marks`)), h("option", { value: "" }, "no marks"));
        this.body = h("div", { class: "panels" });
        this.element = h("div", { class: "inspector" }, h("div", { class: "inspector-head" }, this.choice), this.body);
    }
    /** What to draw over the maze at the shown turn. */
    overlay() {
        if (this.marksOf === null)
            return null;
        const panel = this.panels[this.marksOf];
        return panel ? { player: this.marksOf, marks: panel.marks } : null;
    }
    show(turn) {
        this.panels = inspect(this.data, turn);
        replace(this.body, this.panels.map((panel) => this.panel(panel, turn)));
    }
    panel(panel, turn) {
        return h("section", { class: `panel player-${panel.player}` }, h("h3", {}, h("span", { class: `chip player-${panel.player}` }, h("b", {}, panel.title.slice(0, 1)), panel.title.slice(1)), h("span", { class: "muted" }, turn === 0 ? " startup" : ` turn ${turn}`)), h("dl", {}, panel.fields.map((f) => [
            h("dt", {}, f.label),
            h("dd", { class: `tone-${f.tone}` }, f.value),
        ])), panel.invalid ? h("p", { class: "tone-bad" }, `unreadable answer: ${panel.invalid}`) : null, panel.error
            ? h("div", { class: "bot-error" }, h("p", { class: "tone-bad" }, panel.error.message), panel.error.traceback ? h("pre", { class: "scroll" }, panel.error.traceback) : null)
            : null, panel.events.length
            ? h("ul", { class: "events" }, panel.events.map((e) => h("li", {}, e)))
            : null, panel.log.length
            ? [h("h4", {}, "log"), h("pre", { class: "scroll log" }, panel.log.join("\n"))]
            : null, panel.marks.length
            ? [
                h("h4", {}, "marks"),
                h("ul", { class: "marks" }, panel.marks.map((m) => h("li", {}, swatch(m, panel.player), describeMark(m)))),
            ]
            : null, panel.exit
            ? [
                h("h4", {}, "exit"),
                h("p", {}, panel.exit.summary),
                panel.exit.stderr ? h("pre", { class: "scroll log" }, panel.exit.stderr) : null,
            ]
            : null);
    }
}
/** `route: a path through 12 cells`, `targets: 3 cells`. */
export function describeMark(mark) {
    const n = mark.cells.length;
    const cells = `${n} ${n === 1 ? "cell" : "cells"}`;
    return `${mark.label ?? "unlabelled"}: ${mark.kind === "path" ? `a path through ${cells}` : cells}`;
}
/** A square of a mark's colour, or a short line for a path (styled through
 * the style object: the page's policy forbids style attributes). */
function swatch(mark, player) {
    const sample = h("span", { class: mark.kind === "path" ? "swatch line" : "swatch" });
    sample.style.background = mark.color
        ? `var(--mark-${mark.color})`
        : `var(--player-${player === 0 ? "a" : "b"})`;
    return sample;
}
