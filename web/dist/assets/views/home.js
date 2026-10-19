// The runs and tournaments under the browsed directory.
import { h, replace } from "../dom.js";
import { routeHash } from "../router.js";
import { message } from "./view.js";
export function home(container, ctx) {
    if (ctx.session.mode !== "browse") {
        ctx.navigate({ page: "live" });
        return {};
    }
    replace(container, h("p", { class: "muted" }, "Loading…"));
    ctx.api.runs().then((listing) => {
        const items = listing.entries.map((entry) => entry.kind === "game"
            ? h("li", {}, h("a", { href: routeHash({ page: "game", path: entry.path, turn: null }) }, entry.path), entry.error ? h("span", { class: "error-text" }, ` ${entry.error}`) : null)
            : h("li", {}, h("a", { href: routeHash({ page: "tournament", path: entry.path }) }, entry.path), entry.error ? h("span", { class: "error-text" }, ` ${entry.error}`) : null));
        replace(container, h("h2", {}, "Runs"), items.length ? h("ul", {}, items) : h("p", { class: "muted" }, "Nothing here yet."));
    }, (error) => ctx.error(message(error)));
    return {};
}
