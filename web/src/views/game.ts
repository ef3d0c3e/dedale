// One recorded game.

import { h, replace } from "../dom.ts";
import { fromRecord } from "../game/data.ts";
import { GameScreen } from "../game/screen.ts";
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
  let screen: GameScreen | null = null;
  let disposed = false;
  ctx.api.replay(path).then(
    (record) => {
      if (disposed) return;
      const data = fromRecord(record);
      screen = new GameScreen(data, {
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
        screen.element,
      );
      screen.update();
    },
    (error) => ctx.error(message(error)),
  );
  return {
    dispose() {
      disposed = true;
      screen?.dispose();
    },
  };
}
