// One game as the game view shows it: recorded, or being played.

import { GameModel } from "../model.ts";
import type {
  ConfigRecord,
  EventRecord,
  GameRecord,
  GameResult,
  LiveGame,
  PlayerRecord,
  PlayerStart,
  PlayerStats,
  StartupRecord,
} from "../types.ts";

export interface GameEndData {
  result: GameResult;
  stats: PlayerStats[];
  players: PlayerRecord[];
}

export interface GameData {
  code: string | null;
  config: ConfigRecord;
  players: PlayerStart[];
  model: GameModel;
  /** The latest startup report per player. */
  startups: (StartupRecord | null)[];
  events: EventRecord[];
  /** Once the game is over. */
  end: GameEndData | null;
}

export function fromRecord(record: GameRecord): GameData {
  return {
    code: record.code,
    config: record.config,
    players: record.players.map(({ name, path, about, start }) => ({ name, path, about, start })),
    model: GameModel.fromRecord(record),
    startups: record.players.map((p) => p.startup),
    events: record.events,
    end: { result: record.result, stats: record.stats, players: record.players },
  };
}

/** A live game; `sync` brings it up to date as events arrive. */
export function fromLive(game: LiveGame): GameData {
  const data: GameData = {
    code: game.start.code,
    config: game.start.config,
    players: game.start.players,
    model: new GameModel(
      game.start.maze,
      game.start.players.map((p) => p.start),
    ),
    startups: game.startups,
    events: [],
    end: null,
  };
  sync(data, game);
  return data;
}

/** Adds what a live game gained since. */
export function sync(data: GameData, game: LiveGame): void {
  for (const turn of game.turns.slice(data.model.length)) data.model.push(turn);
  data.startups = game.startups;
  if (game.end && !data.end) {
    data.events = game.end.events;
    data.end = { result: game.end.result, stats: game.end.stats, players: game.end.players };
  }
}
