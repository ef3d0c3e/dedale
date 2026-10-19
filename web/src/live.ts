// A live session as the page sees it: the snapshot it connected with,
// then every event applied in order. Finished games become the records
// Dedale wrote (`record`), the game being played grows turn by turn.

import type { GameRecord, LiveEvent, LiveGame, Pace, Session, Standing } from "./types.ts";

export class LiveState {
  session: Session | null = null;
  pace: Pace = { paused: false, delay_ms: 0 };
  games: LiveGame[] = [];
  standings: Standing[] | null = null;

  /** Applies one event; returns the game it concerns, if any. */
  apply(event: LiveEvent): LiveGame | null {
    switch (event.name) {
      case "snapshot":
        this.session = event.data.session;
        this.pace = event.data.pace;
        this.games = event.data.games;
        this.standings = event.data.standings;
        return null;
      case "waiting":
        this.setPhase("waiting");
        return null;
      case "game_start": {
        const { game, ...start } = event.data;
        const live: LiveGame = {
          game,
          start,
          startups: start.players.map(() => null),
          turns: [],
          end: null,
        };
        this.games = this.games.filter((g) => g.game !== game).concat(live);
        this.setPhase("running");
        return live;
      }
      case "startup": {
        const live = this.game(event.data.game);
        if (live) live.startups[event.data.player] = event.data.startup;
        return live;
      }
      case "turn": {
        const live = this.game(event.data.game);
        // A turn already in the snapshot is not added twice.
        if (live && event.data.turn.turn === live.turns.length + 1)
          live.turns.push(event.data.turn);
        return live;
      }
      case "game_end": {
        const { game, ...end } = event.data;
        const live = this.game(game);
        if (live) live.end = end;
        return live;
      }
      case "standings":
        this.standings = event.data.standings;
        return null;
      case "pace":
        this.pace = event.data;
        return null;
      case "finished":
        this.setPhase("finished");
        if (this.session) this.session.file = event.data.file;
        return null;
      case "error":
        this.setPhase("failed");
        if (this.session) this.session.error = event.data.message;
        return null;
    }
  }

  game(index: number): LiveGame | null {
    return this.games.find((g) => g.game === index) ?? null;
  }

  /** The game being played, or the last one. */
  get current(): LiveGame | null {
    return this.games.at(-1) ?? null;
  }

  private setPhase(phase: NonNullable<Session["phase"]>): void {
    if (this.session) this.session.phase = phase;
  }
}

/**
 * The record of a game that ended, from its start, turns and end: what
 * Dedale wrote to its `replay.json`. `null` while it is played, or when
 * its turns were not streamed (it ended before the page connected).
 */
export function record(game: LiveGame): GameRecord | null {
  const end = game.end;
  if (!end || game.turns.length !== end.result.turns) return null;
  return {
    format: end.format,
    dedale: end.dedale,
    code: game.start.code,
    maze: game.start.maze,
    config: game.start.config,
    players: end.players,
    turns: game.turns,
    events: end.events,
    result: end.result,
    stats: end.stats,
  };
}
