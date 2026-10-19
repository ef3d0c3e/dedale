// The app shell: header, error banner, and the view for the current route.

import { Api, ApiError } from "./api.ts";
import { h, replace } from "./dom.ts";
import { parseRoute, type Route, routeHash } from "./router.ts";
import type { Session } from "./types.ts";
import { game } from "./views/game.ts";
import { home } from "./views/home.ts";
import { live } from "./views/live.ts";
import { tournament } from "./views/tournament.ts";
import type { Context, View } from "./views/view.ts";
import { message } from "./views/view.ts";

function mount(route: Route, container: HTMLElement, ctx: Context): View {
  switch (route.page) {
    case "home":
      return home(container, ctx);
    case "game":
      return game(container, ctx, route.path, route.turn);
    case "tournament":
      return tournament(container, ctx, route.path);
    case "live":
      return live(container, ctx);
    case "unknown":
      replace(container, h("p", {}, "Nothing here. ", h("a", { href: "#/" }, "Back to the start")));
      return {};
  }
}

export async function boot(root: HTMLElement): Promise<void> {
  const api = Api.fromLocation(location);
  const title = h("span", { class: "title" });
  const banner = h("div", { class: "banner", role: "alert", hidden: true });
  const main = h("main");
  replace(
    root,
    h("header", { class: "top" }, h("a", { class: "brand", href: "#/" }, "Dedale"), title),
    banner,
    main,
  );
  const error = (text: string): void => {
    banner.textContent = text;
    banner.hidden = false;
  };

  let session: Session;
  try {
    session = await api.session();
  } catch (e) {
    error(
      e instanceof ApiError && e.status === 403
        ? "This page needs the address Dedale printed: it carries the session's token."
        : `Cannot reach Dedale: ${message(e)}`,
    );
    return;
  }
  title.textContent = session.title;
  document.title = `${session.title} · Dedale`;

  let view: View = {};
  const ctx: Context = {
    api,
    session,
    navigate(route) {
      location.hash = routeHash(route);
    },
    error,
  };
  const show = (): void => {
    view.dispose?.();
    banner.hidden = true;
    replace(main);
    view = mount(parseRoute(location.hash), main, ctx);
  };
  window.addEventListener("hashchange", show);
  show();
}
