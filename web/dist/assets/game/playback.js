// Playing a game back at a number of turns per second.
/** Speeds offered, in turns per second. */
export const SPEEDS = [1, 2, 5, 10, 25, 50, 100, 250, 1000];
export const DEFAULT_SPEED = 10;
/** Where playback is after `elapsed` more milliseconds. */
export function advance(turn, carry, elapsed, speed, last) {
    const total = carry + Math.max(0, elapsed);
    const perTurn = 1000 / speed;
    const steps = Math.floor(total / perTurn);
    const next = Math.min(last, turn + steps);
    const done = next >= last;
    return { turn: next, carry: done ? 0 : total - steps * perTurn, done };
}
/** `turn + delta`, kept within the game. */
export function step(turn, delta, last) {
    return Math.max(0, Math.min(last, turn + delta));
}
