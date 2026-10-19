// One tournament.

import { h, replace } from "../dom.ts";
import type { Context, View } from "./view.ts";
import { message } from "./view.ts";

export function tournament(container: HTMLElement, ctx: Context, path: string): View {
  replace(container, h("p", { class: "muted" }, "Loading…"));
  ctx.api.tournament(path).then(
    (report) => {
      replace(
        container,
        h("h2", {}, `${report.mode} tournament`),
        h(
          "ol",
          {},
          report.leaderboard.map((s) => h("li", {}, s.name)),
        ),
      );
    },
    (error) => ctx.error(message(error)),
  );
  return {};
}
