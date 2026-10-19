// The live session: what is being played.
import { h, replace } from "../dom.js";
import { LiveState } from "../live.js";
import { message } from "./view.js";
export function live(container, ctx) {
    if (ctx.session.mode === "browse") {
        ctx.navigate({ page: "home" });
        return {};
    }
    const state = new LiveState();
    const status = h("p", { class: "muted" }, "Connecting…");
    replace(container, h("h2", {}, ctx.session.title), status);
    const render = () => {
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
            if (!up)
                status.textContent = "Reconnecting…";
        },
        invalid(error) {
            ctx.error(message(error));
        },
    });
    return { dispose: close };
}
