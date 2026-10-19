// The app shell: header, error banner, and the view for the current route.
import { Api, ApiError } from "./api.js";
import { h, replace } from "./dom.js";
import { parseRoute, routeHash } from "./router.js";
import { game } from "./views/game.js";
import { home } from "./views/home.js";
import { live } from "./views/live.js";
import { tournament } from "./views/tournament.js";
import { message } from "./views/view.js";
function mount(route, container, ctx) {
    switch (route.page) {
        case "home":
            return home(container, ctx);
        case "game":
            return game(container, ctx, route.path);
        case "tournament":
            return tournament(container, ctx, route.path);
        case "live":
            return live(container, ctx);
        case "unknown":
            replace(container, h("p", {}, "Nothing here. ", h("a", { href: "#/" }, "Back to the start")));
            return {};
    }
}
export async function boot(root) {
    const api = Api.fromLocation(location);
    const title = h("span", { class: "title" });
    const banner = h("div", { class: "banner", role: "alert", hidden: true });
    const main = h("main");
    replace(root, h("header", { class: "top" }, h("a", { class: "brand", href: "#/" }, "Dedale"), title), banner, main);
    const error = (text) => {
        banner.textContent = text;
        banner.hidden = false;
    };
    let session;
    try {
        session = await api.session();
    }
    catch (e) {
        error(e instanceof ApiError && e.status === 403
            ? "This page needs the address Dedale printed: it carries the session's token."
            : `Cannot reach Dedale: ${message(e)}`);
        return;
    }
    title.textContent = session.title;
    document.title = `${session.title} · Dedale`;
    let view = {};
    const ctx = {
        api,
        session,
        navigate(route) {
            location.hash = routeHash(route);
        },
        error,
    };
    const show = () => {
        view.dispose?.();
        banner.hidden = true;
        replace(main);
        view = mount(parseRoute(location.hash), main, ctx);
    };
    window.addEventListener("hashchange", show);
    show();
}
