// Where maze cells go on the canvas, in CSS pixels.
/** Cells are never drawn larger, so small mazes stay readable. */
export const MAX_CELL = 40;
/** The largest whole cell size fitting `cols × rows` cells in the space. */
export function fit(cols, rows, maxWidth, maxHeight) {
    const cell = Math.max(1, Math.min(MAX_CELL, Math.floor(Math.min(maxWidth / cols, maxHeight / rows))));
    return { cols, rows, cell, width: cell * cols, height: cell * rows };
}
/** The cell under a point of the canvas, if any. */
export function cellAt(layout, px, py) {
    const x = Math.floor(px / layout.cell);
    const y = Math.floor(py / layout.cell);
    return px >= 0 && py >= 0 && x < layout.cols && y < layout.rows ? [x, y] : null;
}
/** The centre of a cell. */
export function cellCentre(layout, x, y) {
    return [(x + 0.5) * layout.cell, (y + 0.5) * layout.cell];
}
/** The top-left corner of a cell. */
export function cellOrigin(layout, x, y) {
    return [x * layout.cell, y * layout.cell];
}
