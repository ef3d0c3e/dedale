// Tables from plain strings (results.ts), with optional links per cell.

import { type Child, h } from "./dom.ts";
import { SPREAD_HELP } from "./format.ts";
import type { Table } from "./results.ts";

/** `cell(row, column, text)` may wrap a cell's text (in a link, a chip). */
export function table(
  data: Table,
  cell: (row: number, column: number, text: string) => Child = (_r, _c, text) => text,
): HTMLTableElement {
  return h(
    "table",
    {},
    h(
      "thead",
      {},
      h(
        "tr",
        {},
        data.headers.map((t) =>
          h("th", t.endsWith("avg / 5% / 1%") ? { title: SPREAD_HELP } : {}, t),
        ),
      ),
    ),
    h(
      "tbody",
      {},
      data.rows.map((row, r) =>
        h(
          "tr",
          {},
          row.map((text, c) =>
            h("td", data.numeric.includes(c) ? { class: "num" } : {}, cell(r, c, text)),
          ),
        ),
      ),
    ),
  );
}
