// What every view gets, and gives back.

import type { Api } from "../api.ts";
import type { Route } from "../router.ts";
import type { Session } from "../types.ts";

export interface Context {
  api: Api;
  session: Session;
  navigate(route: Route): void;
  /** Shows a message above the view. */
  error(message: string): void;
}

/** A mounted view; `dispose` stops what it started (streams, timers). */
export interface View {
  dispose?(): void;
}

/** The text of an error, for the banner. */
export function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
