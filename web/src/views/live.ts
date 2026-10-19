// The live session: what is being played.

import { h, replace } from "../dom.ts";
import { LiveState } from "../live.ts";
import type { Context, View } from "./view.ts";
import { message } from "./view.ts";

export function live(container: HTMLElement, ctx: Context): View {
  if (ctx.session.mode === "browse") {
    ctx.navigate({ page: "home" });
    return {};
  }
  const state = new LiveState();
  const status = h("p", { class: "muted" }, "Connecting…");
  replace(container, h("h2", {}, ctx.session.title), status);
  const render = (): void => {
    const game = state.current;
    const phase = state.session?.phase ?? "waiting";
    status.textContent = game ? `${phase}: game ${game.game}, turn ${game.turns.length}` : phase;
  };
  const close = ctx.api.events({
    event(event) {
      state.apply(event);
      render();
    },
    connected(up) {
      if (!up) status.textContent = "Reconnecting…";
    },
    invalid(error) {
      ctx.error(message(error));
    },
  });
  return { dispose: close };
}
