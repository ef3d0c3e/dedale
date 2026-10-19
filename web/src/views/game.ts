// One recorded game.

import { h, replace } from "../dom.ts";
import { GameModel } from "../model.ts";
import type { Context, View } from "./view.ts";
import { message } from "./view.ts";

export function game(container: HTMLElement, ctx: Context, path: string): View {
  replace(container, h("p", { class: "muted" }, "Loading…"));
  ctx.api.replay(path).then(
    (record) => {
      const model = GameModel.fromRecord(record);
      replace(
        container,
        h("h2", {}, record.players.map((p) => p.name).join(" vs ")),
        h("p", {}, `${record.code ?? "map"}: ${record.result.reason} after ${model.length} turns`),
      );
    },
    (error) => ctx.error(message(error)),
  );
  return {};
}
