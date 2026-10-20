// What bots say about themselves: their author and
// description, next to their name.
import { h } from "./dom.js";
import { letter } from "./format.js";
/** Whether a bot says anything about itself besides its name. */
export function saysMore(about) {
    return about.author !== null || about.description !== null;
}
/**
 * One line per bot: its name, author and description; `null` when no bot
 * says more than its name. `players` labels them A and B, as in a game.
 */
export function aboutList(bots, players) {
    if (!bots.some((b) => saysMore(b.about)))
        return null;
    return h("ul", { class: "about" }, bots.map((b, i) => h("li", {}, players
        ? h("span", { class: `chip player-${i}` }, h("b", {}, letter(i)), ` ${b.name}`)
        : h("b", {}, b.name), b.about.author === null ? null : h("span", { class: "muted" }, ` by ${b.about.author}`), b.about.description === null
        ? null
        : h("span", { class: "description" }, b.about.description))));
}
