// Runs the `dedale` binary the tests drive: `DEDALE_BIN`, or the debug
// build (which `cargo test --all-features` makes, with the fake bot).

import { type ChildProcess, spawn } from "node:child_process";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

/** The repository, from `web/build/e2e/`. */
export const repo = fileURLToPath(new URL("../../../", import.meta.url));

export function binary(name: "dedale" | "dedale-fake-bot"): string {
  if (name === "dedale" && process.env["DEDALE_BIN"]) return process.env["DEDALE_BIN"];
  return `${repo}target/debug/${name}`;
}

export class Dedale {
  readonly process: ChildProcess;
  /** The app's address, as printed. */
  readonly url: string;
  private output = "";

  private constructor(process: ChildProcess, url: string, output: string) {
    this.process = process;
    this.url = url;
    this.output = output;
    process.stdout?.on("data", (data: Buffer) => {
      this.output += data.toString();
    });
  }

  /** Starts `dedale args...` and waits for its address. */
  static async start(args: string[]): Promise<Dedale> {
    const child = spawn(binary("dedale"), args, { stdio: ["ignore", "pipe", "inherit"] });
    const { url, output } = await new Promise<{ url: string; output: string }>(
      (resolve, reject) => {
        let text = "";
        const timer = setTimeout(() => reject(new Error(`no address printed: ${text}`)), 15_000);
        const onData = (data: Buffer): void => {
          text += data.toString();
          const found = text.match(/^view {4}(\S+)$/m);
          if (found?.[1]) {
            clearTimeout(timer);
            child.stdout?.off("data", onData);
            resolve({ url: found[1], output: text });
          }
        };
        child.stdout?.on("data", onData);
        child.on("exit", (code) => reject(new Error(`dedale exited (${code}): ${text}`)));
      },
    );
    return new Dedale(child, url, output);
  }

  get stdout(): string {
    return this.output;
  }

  /** The processes it started (its bots), from /proc. */
  children(): number[] {
    const pid = this.process.pid;
    try {
      return readFileSync(`/proc/${pid}/task/${pid}/children`, "utf8")
        .split(/\s+/)
        .filter(Boolean)
        .map(Number);
    } catch {
      return [];
    }
  }

  /** Ctrl-C; the exit code. */
  async stop(): Promise<number | null> {
    if (this.process.exitCode !== null) return this.process.exitCode;
    const exited = new Promise<number | null>((resolve) =>
      this.process.on("exit", (code) => resolve(code)),
    );
    this.process.kill("SIGINT");
    return exited;
  }
}
