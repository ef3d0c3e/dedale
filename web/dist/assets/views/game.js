// One recorded game.
import { h, replace } from "../dom.js";
import { fromRecord } from "../game/data.js";
import { GameScreen } from "../game/screen.js";
import { routeHash } from "../router.js";
import { message } from "./view.js";
export function game(container, ctx, path, turn) {
    replace(container, h("p", { class: "muted" }, "Loading…"));
    let screen = null;
    let disposed = false;
    ctx.api.replay(path).then((record) => {
        if (disposed)
            return;
        const data = fromRecord(record);
        screen = new GameScreen(data, {
            turn,
            onTurn(shown) {
                // Keeps the address on this turn without reloading the view.
                history.replaceState(null, "", routeHash({ page: "game", path, turn: shown }));
            },
        });
        replace(container, h("h2", {}, data.players.map((p) => p.name).join(" vs ")), h("p", { class: "muted path" }, path), screen.element);
        screen.update();
    }, (error) => ctx.error(message(error)));
    return {
        dispose() {
            disposed = true;
            screen?.dispose();
        },
    };
}
