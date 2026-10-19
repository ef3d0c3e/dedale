// The run browser: games and tournaments under the browsed directory.
import { h, replace } from "../dom.js";
import { filterEntries, formatTime, gameOutcome, seatingName, sortEntries, tournamentOutcome, } from "../results.js";
import { routeHash } from "../router.js";
import { message } from "./view.js";
function row(entry) {
    const link = entry.kind === "game"
        ? routeHash({ page: "game", path: entry.path, turn: null })
        : routeHash({ page: "tournament", path: entry.path });
    const kind = entry.kind === "game" ? seatingName(entry.seating) : `${entry.mode ?? "?"} tournament`;
    const outcome = entry.kind === "game" ? gameOutcome(entry) : tournamentOutcome(entry);
    const code = entry.kind === "game" ? (entry.code ?? (entry.error ? "-" : "map")) : "";
    return h("tr", { class: entry.error ? "broken" : null }, h("td", { class: "nowrap" }, formatTime(entry.modified)), h("td", {}, h("a", { href: link }, entry.bots.join(" vs ") || entry.path)), h("td", {}, kind), h("td", { class: entry.error ? "error-text" : null }, outcome), h("td", { class: "muted mono" }, code), h("td", { class: "muted mono" }, entry.path));
}
export function home(container, ctx) {
    if (ctx.session.mode !== "browse") {
        ctx.navigate({ page: "live" });
        return {};
    }
    replace(container, h("p", { class: "muted" }, "Loading…"));
    ctx.api.runs().then((listing) => {
        const entries = sortEntries(listing.entries);
        const body = h("tbody");
        const count = h("span", { class: "muted" });
        const filter = h("input", {
            type: "search",
            placeholder: "filter by bot",
            "aria-label": "Filter by bot",
            oninput: () => show(),
        });
        const show = () => {
            const shown = filterEntries(entries, filter.value);
            replace(body, shown.map(row));
            count.textContent = `${shown.length} of ${entries.length}`;
        };
        replace(container, h("div", { class: "toolbar" }, h("h2", {}, "Runs"), filter, count), entries.length
            ? h("div", { class: "scroll-x" }, h("table", { class: "runs" }, h("thead", {}, h("tr", {}, ["when", "bots", "kind", "result", "maze", "file"].map((t) => h("th", {}, t)))), body))
            : h("p", { class: "muted" }, `Nothing here yet: dedale run and dedale tournament write their results under ${ctx.session.title}.`), listing.truncated
            ? h("p", { class: "muted" }, "Only part of the directory was searched: it is very large.")
            : null);
        show();
    }, (error) => ctx.error(message(error)));
    return {};
}
