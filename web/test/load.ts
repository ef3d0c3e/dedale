// Fixtures written by the Rust test `web_fixtures` from the golden replay.

import { readFileSync } from "node:fs";

/** `test/fixtures/<name>`, parsed (tests run from `build/test/`). */
export function fixture(name: string): unknown {
  return JSON.parse(readFileSync(new URL(`../../test/fixtures/${name}`, import.meta.url), "utf8"));
}

/** A JSON value tests reach into freely to damage it. */
// biome-ignore lint/suspicious/noExplicitAny: damaging arbitrary fields is the point.
export type Loose = any;

/** A deep copy of `value` with `change` applied, for damaged inputs. */
export function damaged<T>(value: T, change: (copy: Loose) => void): unknown {
  const copy = structuredClone(value);
  change(copy);
  return copy;
}
