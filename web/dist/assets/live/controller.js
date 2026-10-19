// What the live page shows, apart from drawing it: the session as events
// left it, which game to show, whether the view follows the newest turn,
// and the pace choices.
import { LiveState } from "../live.js";
/** Delays between turns offered, in milliseconds. */
export const DELAYS = [0, 10, 25, 50, 100, 250, 500, 1000, 2000];
/** The offered delay closest to `ms`. */
export function nearestDelay(ms) {
    return DELAYS.reduce((best, d) => (Math.abs(d - ms) < Math.abs(best - ms) ? d : best), 0);
}
export function paceLabel(pace) {
    if (pace.paused)
        return "paused";
    return pace.delay_ms === 0 ? "full speed" : `${pace.delay_ms} ms between turns`;
}
export class LiveController {
    state = new LiveState();
    /** The view tracks the newest turn. */
    following = true;
    connected = false;
    apply(event) {
        this.state.apply(event);
    }
    /** The game to show: the one being played, or the last one. */
    get game() {
        return this.state.current;
    }
    /** Whether the game's turns are here, or must come from its replay (it
     * ended before this page connected). */
    needsReplay(game) {
        return game.end !== null && game.turns.length < game.end.result.turns;
    }
    /** The user showed `turn` of `last`: following stops unless it is the last. */
    userMoved(turn, last) {
        this.following = turn >= last;
    }
    /** Where the view goes when turns arrive: the newest, if following. */
    target(last) {
        return this.following ? last : null;
    }
    /** Follow again; the turn to go to. */
    resume(last) {
        this.following = true;
        return last;
    }
    get waiting() {
        return (this.state.session?.phase ?? "waiting") === "waiting";
    }
}
