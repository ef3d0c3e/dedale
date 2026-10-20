// What bots say about themselves: their version, author and description,
// next to their name.
import { h } from "./dom.js";
import { letter } from "./format.js";
/** Whether a bot says anything about itself besides its name. */
export function saysMore(about) {
    return about.author !== null || about.description !== null || about.version !== null;
}
/** `Scorer v3`, or the name alone without a version. */
export function versioned(name, about) {
    return about.version === null ? name : `${name} v${about.version}`;
}
/**
 * One line per bot: its name and version, author and description; `null`
 * when no bot says more than its name. `players` labels them A and B, as in
 * a game.
 */
export function aboutList(bots, players) {
    if (!bots.some((b) => saysMore(b.about)))
        return null;
    return h("ul", { class: "about" }, bots.map((b, i) => h("li", {}, players
        ? h("span", { class: `chip player-${i}` }, h("b", {}, letter(i)), ` ${versioned(b.name, b.about)}`)
        : h("b", {}, versioned(b.name, b.about)), b.about.author === null ? null : h("span", { class: "muted" }, ` by ${b.about.author}`), b.about.description === null
        ? null
        : h("span", { class: "description" }, b.about.description))));
}
