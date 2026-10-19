// One tournament.
import { h, replace } from "../dom.js";
import { message } from "./view.js";
export function tournament(container, ctx, path) {
    replace(container, h("p", { class: "muted" }, "Loading…"));
    ctx.api.tournament(path).then((report) => {
        replace(container, h("h2", {}, `${report.mode} tournament`), h("ol", {}, report.leaderboard.map((s) => h("li", {}, s.name))));
    }, (error) => ctx.error(message(error)));
    return {};
}
