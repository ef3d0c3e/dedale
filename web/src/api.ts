// The local server's API: every call carries the token
// from the page's own address.

import type { GameRecord, Listing, LiveEvent, Pace, Session, TournamentReport } from "./types.ts";
import { apiUrl } from "./url.ts";
import {
  pace,
  parseListing,
  parseLiveEvent,
  parseReplay,
  parseTournament,
  session,
} from "./validate.ts";

export class ApiError extends Error {
  override name = "ApiError";
  readonly status: number;

  constructor(status: number, message: string) {
    super(message);
    this.status = status;
  }
}

/** Live event names, as the server sends them. */
export const EVENT_NAMES = [
  "snapshot",
  "waiting",
  "game_start",
  "startup",
  "turn",
  "game_end",
  "standings",
  "pace",
  "finished",
  "error",
] as const;

export interface EventHandlers {
  event(event: LiveEvent): void;
  /** The stream is up (a snapshot follows) or down (it reconnects). */
  connected(up: boolean): void;
  /** An event that does not parse. */
  invalid(error: Error): void;
}

/** Reconnects this long after a stream the browser gave up on. */
const RECONNECT_MS = 2000;

export class Api {
  readonly token: string;

  constructor(token: string) {
    this.token = token;
  }

  /** The token from `?token=` in the page's address. */
  static fromLocation(location: { search: string }): Api {
    return new Api(new URLSearchParams(location.search).get("token") ?? "");
  }

  url(path: string, params: Record<string, string> = {}): string {
    return apiUrl(path, this.token, params);
  }

  private async request(
    method: string,
    path: string,
    params: Record<string, string>,
    body?: unknown,
  ) {
    const init: RequestInit = { method };
    if (body !== undefined) {
      init.headers = { "Content-Type": "application/json" };
      init.body = JSON.stringify(body);
    }
    const response = await fetch(this.url(path, params), init);
    const text = await response.text();
    let data: unknown = null;
    try {
      data = text ? JSON.parse(text) : null;
    } catch {
      throw new ApiError(response.status, `${path}: the answer is not JSON`);
    }
    if (!response.ok) {
      const message =
        typeof data === "object" && data !== null && "error" in data
          ? String(data.error)
          : response.statusText;
      throw new ApiError(response.status, message);
    }
    return data;
  }

  async get(path: string, params: Record<string, string> = {}): Promise<unknown> {
    return this.request("GET", path, params);
  }

  async post(path: string, body?: unknown): Promise<unknown> {
    return this.request("POST", path, {}, body);
  }

  async session(): Promise<Session> {
    return session(await this.get("/api/session"), "session");
  }

  async runs(): Promise<Listing> {
    return parseListing(await this.get("/api/runs"));
  }

  async replay(path: string): Promise<GameRecord> {
    return parseReplay(await this.get("/api/file", { path }));
  }

  async tournament(path: string): Promise<TournamentReport> {
    return parseTournament(await this.get("/api/file", { path }));
  }

  async start(): Promise<void> {
    await this.post("/api/start");
  }

  async control(change: Partial<Pace>): Promise<Pace> {
    return pace(await this.post("/api/control", change), "pace");
  }

  /**
   * Follows the live stream until the returned function is called. Every
   * connection starts with a snapshot, so reconnecting catches up.
   */
  events(handlers: EventHandlers): () => void {
    let source: EventSource | null = null;
    let timer: ReturnType<typeof setTimeout> | null = null;
    let closed = false;
    const connect = (): void => {
      source = new EventSource(this.url("/api/events"));
      source.onopen = () => handlers.connected(true);
      source.onerror = () => {
        handlers.connected(false);
        // The browser retries on its own unless it gave up.
        if (source?.readyState === EventSource.CLOSED && !closed) {
          timer = setTimeout(connect, RECONNECT_MS);
        }
      };
      for (const name of EVENT_NAMES) {
        source.addEventListener(name, (message) => {
          try {
            handlers.event(
              parseLiveEvent(name, JSON.parse((message as MessageEvent<string>).data)),
            );
          } catch (error) {
            handlers.invalid(error instanceof Error ? error : new Error(String(error)));
          }
        });
      }
    };
    connect();
    return () => {
      closed = true;
      if (timer !== null) clearTimeout(timer);
      source?.close();
    };
  }
}
