// Tables from plain strings (results.ts), with optional links per cell.
import { h } from "./dom.js";
/** `cell(row, column, text)` may wrap a cell's text (in a link, a chip). */
export function table(data, cell = (_r, _c, text) => text) {
    return h("table", {}, h("thead", {}, h("tr", {}, data.headers.map((t) => h("th", {}, t)))), h("tbody", {}, data.rows.map((row, r) => h("tr", {}, row.map((text, c) => h("td", data.numeric.includes(c) ? { class: "num" } : {}, cell(r, c, text)))))));
}
