// The live session: Start, pace, and the game being played (or the
// tournament around it), following the newest turn.

import { h, replace } from "../dom.ts";
import { describeResult } from "../format.ts";
import { fromLive, fromRecord, type GameData, sync } from "../game/data.ts";
import { GameScreen } from "../game/screen.ts";
import { DELAYS, LiveController, nearestDelay, paceLabel } from "../live/controller.ts";
import { record } from "../live.ts";
import { leaderboard, players } from "../results.ts";
import { routeHash } from "../router.ts";
import { table } from "../table.ts";
import type { LiveGame } from "../types.ts";
import type { Context, View } from "./view.ts";
import { message } from "./view.ts";

/** The game screen refreshes at most this often while turns stream in. */
const REFRESH_MS = 100;

/** The wait between turns is for watching, not the bots' time limit. */
const WAIT_HELP =
  "Slows the game down for watching: Dedale waits this long between turns. " +
  "Bots are not timed while it waits, and their time per move stays as set when starting (--move-ms).";

interface Shown {
  /** 0 for the preview before Start. */
  game: number;
  data: GameData;
  screen: GameScreen;
  /** Turns come from the stream (not from a replay). */
  streamed: boolean;
}

export function live(container: HTMLElement, ctx: Context): View {
  if (ctx.session.mode === "browse") {
    ctx.navigate({ page: "home" });
    return {};
  }
  const controller = new LiveController();
  const tournament = ctx.session.mode === "tournament";
  // For the end-to-end tests, and the curious in the console: a game as
  // this page assembled it from the stream (the latest, or game `n`).
  const hook = {
    record: (n?: number) => {
      const game = n === undefined ? controller.game : controller.state.game(n);
      return game ? record(game) : null;
    },
  };
  Object.assign(globalThis, { dedaleLive: hook });

  const phase = h("span", { class: "phase" });
  const connection = h("span", { class: "connection muted" }, "connecting…");
  const start = h(
    "button",
    {
      type: "button",
      class: "primary",
      onclick: () => {
        start.disabled = true;
        ctx.api.start().catch((e: unknown) => {
          // Another page may have started it first.
          if (!message(e).includes("already started")) {
            start.disabled = false;
            ctx.error(message(e));
          }
        });
      },
    },
    "Start",
  );
  const pause = h("button", {
    type: "button",
    onclick: () => {
      ctx.api
        .control({ paused: !controller.state.pace.paused })
        .catch((e: unknown) => ctx.error(message(e)));
    },
  });
  const delay = h(
    "select",
    {
      "aria-label": "Delay between turns",
      onchange: () => {
        ctx.api
          .control({ delay_ms: Number(delay.value) })
          .catch((e: unknown) => ctx.error(message(e)));
      },
    },
    DELAYS.map((d) => h("option", { value: d }, d === 0 ? "none" : `${d} ms`)),
  );
  const wait = h("label", { class: "wait", title: WAIT_HELP }, "Wait between turns ", delay);
  // The bots' real limit, in sight while the game runs.
  const limit = h("span", { class: "limit muted", title: "A later answer is a timeout" });
  const follow = h(
    "button",
    {
      type: "button",
      onclick: () => {
        if (shown) shown.screen.view.goTo(controller.resume(shown.data.model.length));
        render();
      },
    },
    "Follow live",
  );
  const paceText = h("span", { class: "muted" });
  const notice = h("div", { class: "notice" });
  const intro = h("div", { class: "intro" });
  const standings = h("div", { class: "standings" });
  const gameTitle = h("h3", {});
  const gameBox = h("div", { class: "current" });

  replace(
    container,
    h("div", { class: "toolbar" }, h("h2", {}, ctx.session.title), phase, connection),
    h("div", { class: "live-controls" }, start, pause, wait, paceText, limit, follow),
    notice,
    intro,
    standings,
    gameTitle,
    gameBox,
  );

  let shown: Shown | null = null;
  let frame: number | null = null;
  let lastRefresh = 0;
  let lastPhase = controller.state.session?.phase;
  let disposed = false;

  const show = (game: number, data: GameData, streamed: boolean): void => {
    // What the viewer chose to see stays as the screen is rebuilt: on Start,
    // for each game of a tournament, and for a finished game's replay.
    const settings = shown?.screen.settings;
    shown?.screen.dispose();
    const screen = new GameScreen(data, {
      turn: controller.following ? data.model.length : 0,
      onTurn: (turn) => controller.userMoved(turn, data.model.length),
      settings,
    });
    shown = { game, data, screen, streamed };
    replace(gameBox, screen.element);
    screen.update();
  };

  const loadReplay = (game: LiveGame): void => {
    const end = game.end;
    if (!end) return;
    ctx.api.replay(end.replay).then(
      (record) => {
        if (!disposed && controller.game?.game === game.game)
          show(game.game, fromRecord(record), false);
      },
      (e: unknown) => ctx.error(message(e)),
    );
  };

  /** Brings the game screen up to date with the stream. */
  const refreshGame = (): void => {
    const game = controller.game;
    if (!game) {
      const preview = controller.state.session?.start;
      if (preview && !shown) {
        show(0, fromLive({ game: 0, start: preview, startups: [], turns: [], end: null }), true);
      }
      return;
    }
    if (!shown || shown.game !== game.game) {
      if (controller.needsReplay(game)) loadReplay(game);
      else show(game.game, fromLive(game), true);
      return;
    }
    if (!shown.streamed) return;
    sync(shown.data, game);
    shown.screen.update();
    const target = controller.target(shown.data.model.length);
    if (target !== null && target !== shown.screen.view.current) shown.screen.view.goTo(target);
  };

  const renderTournament = (): void => {
    if (!tournament) return;
    const mode = controller.state.session?.tournament ?? "duo";
    const finished = controller.state.games.filter((g) => g.end);
    replace(
      standings,
      controller.state.standings?.length
        ? [
            h("h3", {}, "Leaderboard"),
            h("div", { class: "scroll-x" }, table(leaderboard(mode, controller.state.standings))),
          ]
        : null,
      finished.length
        ? [
            h("h3", {}, "Games played"),
            h(
              "div",
              { class: "scroll-x" },
              table(
                {
                  headers: ["game", "maze", "players", "result"],
                  numeric: [0],
                  rows: finished.map((g) => [
                    String(g.game),
                    g.start.code ?? "map",
                    players(
                      g.start.players.map((_, i) => i),
                      g.start.players.map((p) => p.name),
                    ),
                    g.end
                      ? describeResult(
                          g.end.result,
                          g.start.config.seating,
                          g.start.players.map((p) => p.name),
                        )
                      : "",
                  ]),
                },
                (row, column, text) => {
                  const replay = finished[row]?.end?.replay;
                  return column === 0 && replay
                    ? h("a", { href: routeHash({ page: "game", path: replay, turn: null }) }, text)
                    : text;
                },
              ),
            ),
          ]
        : null,
    );
  };

  const renderStatus = (): void => {
    const session = controller.state.session;
    const state = session?.phase ?? "waiting";
    phase.textContent = state;
    phase.className = `phase phase-${state}`;
    connection.textContent = controller.connected ? "connected" : "reconnecting…";
    const pace = controller.state.pace;
    start.hidden = state !== "waiting";
    const running = state === "running";
    pause.hidden = wait.hidden = paceText.hidden = !(running || state === "waiting");
    const config = controller.game?.start.config ?? session?.start?.config;
    limit.textContent = config ? `· bots: ${config.move_ms} ms per move` : "";
    pause.textContent = pace.paused ? "Resume" : "Pause";
    delay.value = String(nearestDelay(pace.delay_ms));
    paceText.textContent = paceLabel(pace);
    follow.hidden = controller.following || !shown?.streamed;

    if (state === "waiting") {
      replace(
        intro,
        h("p", {}, `${session?.bots?.join(" vs ") ?? ""} · ${session?.settings ?? ""}`),
        h(
          "p",
          { class: "muted" },
          "The bots start when you press Start. You can pause or slow the game down at any time.",
        ),
      );
    } else replace(intro);

    if (state === "finished") {
      const file = session?.file ?? "";
      replace(
        notice,
        tournament
          ? h(
              "p",
              {},
              "The tournament is over: ",
              h("a", { href: routeHash({ page: "tournament", path: file }) }, "results"),
              ". Dedale keeps serving this page until you stop it.",
            )
          : h("p", {}, "The game is over. Dedale keeps serving this page until you stop it."),
      );
    } else if (state === "failed") {
      replace(
        notice,
        h("p", { class: "error-text" }, `The session failed: ${session?.error ?? "unknown error"}`),
      );
    } else replace(notice);

    const game = controller.game;
    const total = session?.games ?? 1;
    gameTitle.textContent = game && tournament ? `Game ${game.game} of ${total}` : "";
  };

  const render = (): void => {
    frame = null;
    renderStatus();
    const now = performance.now();
    // A new phase (finished, failed) shows everything at once.
    const phaseNow = controller.state.session?.phase;
    if (now - lastRefresh >= REFRESH_MS || !shown || phaseNow !== lastPhase) {
      lastRefresh = now;
      lastPhase = phaseNow;
      refreshGame();
      renderTournament();
    } else {
      schedule();
    }
  };

  const schedule = (): void => {
    if (frame === null && !disposed) frame = requestAnimationFrame(render);
  };

  const close = ctx.api.events({
    event(event) {
      controller.apply(event);
      schedule();
    },
    connected(up) {
      controller.connected = up;
      schedule();
    },
    invalid(error) {
      ctx.error(message(error));
    },
  });
  renderStatus();

  return {
    dispose() {
      disposed = true;
      Reflect.deleteProperty(globalThis, "dedaleLive");
      close();
      if (frame !== null) cancelAnimationFrame(frame);
      shown?.screen.dispose();
    },
  };
}
