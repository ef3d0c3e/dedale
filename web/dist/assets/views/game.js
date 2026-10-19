// One recorded game.
import { h, replace } from "../dom.js";
import { fromRecord } from "../game/data.js";
import { GameView } from "../game/view.js";
import { routeHash } from "../router.js";
import { message } from "./view.js";
export function game(container, ctx, path, turn) {
    replace(container, h("p", { class: "muted" }, "Loading…"));
    let view = null;
    let disposed = false;
    ctx.api.replay(path).then((record) => {
        if (disposed)
            return;
        const data = fromRecord(record);
        view = new GameView(data, {
            turn,
            onTurn(shown) {
                // Keeps the address on this turn without reloading the view.
                history.replaceState(null, "", routeHash({ page: "game", path, turn: shown }));
            },
        });
        replace(container, h("h2", {}, data.players.map((p) => p.name).join(" vs ")), h("p", { class: "muted path" }, path), view.element);
        view.update();
    }, (error) => ctx.error(message(error)));
    return {
        dispose() {
            disposed = true;
            view?.dispose();
        },
    };
}
