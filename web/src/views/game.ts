// One recorded game.

import { h, replace } from "../dom.ts";
import { fromRecord } from "../game/data.ts";
import { GameView } from "../game/view.ts";
import { routeHash } from "../router.ts";
import type { Context, View } from "./view.ts";
import { message } from "./view.ts";

export function game(
  container: HTMLElement,
  ctx: Context,
  path: string,
  turn: number | null,
): View {
  replace(container, h("p", { class: "muted" }, "Loading…"));
  let view: GameView | null = null;
  let disposed = false;
  ctx.api.replay(path).then(
    (record) => {
      if (disposed) return;
      const data = fromRecord(record);
      view = new GameView(data, {
        turn,
        onTurn(shown) {
          // Keeps the address on this turn without reloading the view.
          history.replaceState(null, "", routeHash({ page: "game", path, turn: shown }));
        },
      });
      replace(
        container,
        h("h2", {}, data.players.map((p) => p.name).join(" vs ")),
        h("p", { class: "muted path" }, path),
        view.element,
      );
      view.update();
    },
    (error) => ctx.error(message(error)),
  );
  return {
    dispose() {
      disposed = true;
      view?.dispose();
    },
  };
}
