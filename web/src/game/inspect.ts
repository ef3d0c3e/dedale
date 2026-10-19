// What the turn inspector says about each player at a turn: at turn 0 the
// startup, afterwards the turn's move, outcome, timings, words and marks,
// plus how the bot exited once the game is over. Plain strings only: the
// page shows them as text, never as HTML.

import { formatUs, letter, outcomeName, score } from "../format.ts";
import type { BotError, ExitRecord, Mark, StartupRecord } from "../types.ts";
import type { GameData } from "./data.ts";

export type Tone = "ok" | "bad" | "plain";

export interface Field {
  label: string;
  value: string;
  tone: Tone;
}

export interface PlayerPanel {
  player: number;
  /** `A greedy`. */
  title: string;
  fields: Field[];
  log: string[];
  marks: Mark[];
  error: BotError | null;
  /** Why the answer could not be read. */
  invalid: string | null;
  /** Late and ignored answers noticed on this turn. */
  events: string[];
  /** Once the game is over: how the bot exited, and its last stderr. */
  exit: { summary: string; stderr: string } | null;
}

const plain = (label: string, value: string): Field => ({ label, value, tone: "plain" });

function startupFields(startup: StartupRecord | null, budgetMs: number): Field[] {
  if (!startup) return [{ label: "startup", value: "starting…", tone: "plain" }];
  const words: Record<StartupRecord["outcome"], string> = {
    ok: "ready",
    late: "ready late",
    init_error: "init() raised",
    timeout: "not ready in time",
    crashed: "crashed",
  };
  return [
    {
      label: "startup",
      value: words[startup.outcome],
      tone: startup.outcome === "ok" ? "ok" : "bad",
    },
    plain("ready after", `${formatUs(startup.startup_us)} of ${budgetMs} ms`),
    plain("import + init()", formatUs(startup.init_us)),
  ];
}

export function exitSummary(exit: ExitRecord): string {
  if (exit.killed) {
    return exit.signal === null ? "killed by Dedale" : `killed by Dedale (signal ${exit.signal})`;
  }
  if (exit.signal !== null) return `killed by signal ${exit.signal}`;
  if (exit.code !== null)
    return exit.code === 0 ? "exited normally" : `exited with code ${exit.code}`;
  return "exit unknown";
}

/** Every player's panel at `turn`. */
export function inspect(data: GameData, turn: number): PlayerPanel[] {
  const record = turn > 0 ? data.model.turns[turn - 1] : undefined;
  const over = data.end !== null && turn >= data.model.length;
  return data.players.map((player, i): PlayerPanel => {
    const events = data.events
      .filter((e) => e.turn === turn && e.player === i)
      .map((e) =>
        e.kind === "late" ? `late answer to turn ${e.for_turn}` : `ignored: ${e.reason}`,
      );
    const exitRecord = over ? (data.end?.players[i]?.exit ?? null) : null;
    const exit = exitRecord
      ? { summary: exitSummary(exitRecord), stderr: exitRecord.stderr_tail }
      : null;
    const title = `${letter(i)} ${player.name}`;
    const p = record?.players[i];
    if (!p) {
      const startup = data.startups[i] ?? null;
      return {
        player: i,
        title,
        fields: startupFields(startup, data.config.startup_ms),
        log: startup?.log ?? [],
        marks: startup?.marks ?? [],
        error: startup?.error ?? null,
        invalid: null,
        events,
        exit,
      };
    }
    const requested = p.requested ?? "nothing";
    const move = p.outcome === "ok" ? p.applied : `${requested}, stayed`;
    return {
      player: i,
      title,
      fields: [
        plain("move", move),
        {
          label: "outcome",
          value: outcomeName(p.outcome),
          tone: p.outcome === "ok" ? "ok" : "bad",
        },
        plain(
          "latency",
          p.latency_us === null ? "-" : `${formatUs(p.latency_us)} of ${data.config.move_ms} ms`,
        ),
        plain("in move()", formatUs(p.think_us)),
        plain("position", `(${p.pos[0]}, ${p.pos[1]})`),
        plain(
          "score",
          p.gained > 0 ? `${score(p.half_points)} (+${score(p.gained)})` : score(p.half_points),
        ),
      ],
      log: p.log,
      marks: p.marks,
      error: p.error,
      invalid: p.invalid,
      events,
      exit,
    };
  });
}
