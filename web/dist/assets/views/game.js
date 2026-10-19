// One recorded game.
import { h, replace } from "../dom.js";
import { GameModel } from "../model.js";
import { message } from "./view.js";
export function game(container, ctx, path) {
    replace(container, h("p", { class: "muted" }, "Loading…"));
    ctx.api.replay(path).then((record) => {
        const model = GameModel.fromRecord(record);
        replace(container, h("h2", {}, record.players.map((p) => p.name).join(" vs ")), h("p", {}, `${record.code ?? "map"}: ${record.result.reason} after ${model.length} turns`));
    }, (error) => ctx.error(message(error)));
    return {};
}
