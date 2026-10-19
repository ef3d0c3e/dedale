// One tournament: leaderboard and games.

import { h, replace } from "../dom.ts";
import { describeResult } from "../format.ts";
import { leaderboard, players, replayPath, seatingName, settings } from "../results.ts";
import { routeHash } from "../router.ts";
import { table } from "../table.ts";
import type { TournamentReport } from "../types.ts";
import type { Context, View } from "./view.ts";
import { message } from "./view.ts";

/** The games of a tournament, each linking to its game view. */
export function gamesTable(report: TournamentReport, path: string): HTMLTableElement {
  const names = report.bots.map((b) => b.name);
  return table(
    {
      headers: ["game", "maze", "seating", "players", "result"],
      numeric: [0],
      rows: report.games.map((g) => [
        String(g.index),
        g.code ?? "map",
        seatingName(g.seating),
        players(g.bots, names),
        describeResult(
          g.result,
          g.seating,
          g.bots.map((b) => names[b] ?? "?"),
        ),
      ]),
    },
    (row, column, text) => {
      const game = report.games[row];
      return (column === 0 || column === 4) && game
        ? h(
            "a",
            { href: routeHash({ page: "game", path: replayPath(path, game.replay), turn: null }) },
            text,
          )
        : text;
    },
  );
}

export function tournament(container: HTMLElement, ctx: Context, path: string): View {
  replace(container, h("p", { class: "muted" }, "Loading…"));
  ctx.api.tournament(path).then(
    (report) => {
      replace(
        container,
        h("h2", {}, `${report.mode} tournament: ${report.bots.map((b) => b.name).join(" vs ")}`),
        h("p", { class: "muted path" }, path),
        h("p", { class: "muted" }, settings(report)),
        h("h3", {}, "Leaderboard"),
        h("div", { class: "scroll-x" }, table(leaderboard(report.mode, report.leaderboard))),
        h("h3", {}, "Games"),
        h("div", { class: "scroll-x" }, gamesTable(report, path)),
      );
    },
    (error) => ctx.error(message(error)),
  );
  return {};
}
