import assert from "node:assert/strict";
import { test } from "node:test";

import { apiUrl } from "../src/url.ts";

test("API URLs carry the token", () => {
  assert.equal(apiUrl("/api/session", "ab12"), "/api/session?token=ab12");
});

test("parameters are encoded", () => {
  assert.equal(
    apiUrl("/api/file", "ab12", { path: "run 1/replay.json" }),
    "/api/file?path=run+1%2Freplay.json&token=ab12",
  );
});
