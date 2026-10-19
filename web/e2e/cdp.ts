// A small Chrome DevTools Protocol driver: launches the installed
// Chromium headless and drives pages over the browser's WebSocket (Node's
// built-in one), so the end-to-end tests need no Puppeteer.

import { type ChildProcess, spawn } from "node:child_process";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, join } from "node:path";

/** `DEDALE_CHROMIUM`, or the first Chromium or Chrome on the PATH. */
export function findChromium(): string | null {
  const chosen = process.env["DEDALE_CHROMIUM"];
  if (chosen) return chosen;
  const names = ["chromium", "chromium-browser", "google-chrome", "google-chrome-stable"];
  for (const dir of (process.env["PATH"] ?? "").split(delimiter)) {
    for (const name of names) {
      const path = join(dir, name);
      if (existsSync(path)) return path;
    }
  }
  return null;
}

interface Message {
  id?: number;
  method?: string;
  params?: Record<string, unknown>;
  result?: Record<string, unknown>;
  error?: { message: string };
  sessionId?: string;
}

function within<T>(promise: Promise<T>, ms: number, what: string): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const timeout = new Promise<T>((_, reject) => {
    timer = setTimeout(() => reject(new Error(`timed out: ${what}`)), ms);
  });
  // Cleared once settled, so a pending timer does not keep Node running.
  return Promise.race([promise, timeout]).finally(() => clearTimeout(timer));
}

export class Browser {
  private readonly process: ChildProcess;
  private readonly socket: WebSocket;
  private readonly profile: string;
  private next = 0;
  private readonly pending = new Map<number, (message: Message) => void>();
  private readonly listeners: ((message: Message) => void)[] = [];

  private constructor(process: ChildProcess, socket: WebSocket, profile: string) {
    this.process = process;
    this.socket = socket;
    this.profile = profile;
    socket.addEventListener("message", (event) => {
      const message = JSON.parse(String(event.data)) as Message;
      if (message.id !== undefined) {
        this.pending.get(message.id)?.(message);
        this.pending.delete(message.id);
      } else {
        for (const listener of this.listeners) listener(message);
      }
    });
  }

  static async launch(): Promise<Browser> {
    const executable = findChromium();
    if (!executable) throw new Error("no Chromium found: install one or set DEDALE_CHROMIUM");
    const profile = mkdtempSync(join(tmpdir(), "dedale-e2e-"));
    const child = spawn(
      executable,
      [
        "--headless",
        "--disable-gpu",
        "--no-sandbox",
        "--no-first-run",
        "--hide-scrollbars",
        "--window-size=1300,1000",
        `--user-data-dir=${profile}`,
        "--remote-debugging-port=0",
        "about:blank",
      ],
      { stdio: ["ignore", "ignore", "pipe"] },
    );
    const url = await within(
      new Promise<string>((resolve, reject) => {
        let text = "";
        child.stderr?.on("data", (data: Buffer) => {
          text += data.toString();
          const found = text.match(/ws:\/\/\S+/);
          if (found) resolve(found[0]);
        });
        child.on("exit", (code) => reject(new Error(`${executable} exited (${code}): ${text}`)));
      }),
      15_000,
      "Chromium's DevTools address",
    );
    const socket = new WebSocket(url);
    await within(
      new Promise((resolve, reject) => {
        socket.addEventListener("open", resolve);
        socket.addEventListener("error", reject);
      }),
      5000,
      "the DevTools connection",
    );
    return new Browser(child, socket, profile);
  }

  send(
    method: string,
    params: Record<string, unknown> = {},
    sessionId?: string,
  ): Promise<Record<string, unknown>> {
    const id = ++this.next;
    const reply = new Promise<Record<string, unknown>>((resolve, reject) => {
      this.pending.set(id, (message) =>
        message.error
          ? reject(new Error(`${method}: ${message.error.message}`))
          : resolve(message.result ?? {}),
      );
    });
    this.socket.send(JSON.stringify({ id, method, params, sessionId }));
    return within(reply, 15_000, method);
  }

  onEvent(listener: (message: Message) => void): void {
    this.listeners.push(listener);
  }

  async newPage(): Promise<Page> {
    const { targetId } = await this.send("Target.createTarget", { url: "about:blank" });
    const { sessionId } = await this.send("Target.attachToTarget", { targetId, flatten: true });
    const page = new Page(this, String(sessionId));
    await page.send("Runtime.enable");
    await page.send("Page.enable");
    return page;
  }

  close(): void {
    this.socket.close();
    this.process.kill("SIGKILL");
    rmSync(this.profile, { recursive: true, force: true });
  }
}

export class Page {
  /** Uncaught errors in the page, which the tests expect to stay empty. */
  readonly errors: string[] = [];
  private readonly browser: Browser;
  private readonly session: string;

  constructor(browser: Browser, session: string) {
    this.browser = browser;
    this.session = session;
    browser.onEvent((message) => {
      if (message.sessionId !== session) return;
      if (message.method === "Runtime.exceptionThrown") {
        const details = message.params?.["exceptionDetails"] as {
          text?: string;
          exception?: { description?: string };
        };
        this.errors.push(details.exception?.description ?? details.text ?? "exception");
      }
    });
  }

  send(method: string, params: Record<string, unknown> = {}): Promise<Record<string, unknown>> {
    return this.browser.send(method, params, this.session);
  }

  async goto(url: string): Promise<void> {
    await this.send("Page.navigate", { url });
  }

  /** Evaluates `expression` in the page (promises awaited), by value. */
  async eval<T>(expression: string): Promise<T> {
    const reply = await this.send("Runtime.evaluate", {
      expression,
      awaitPromise: true,
      returnByValue: true,
    });
    const exception = reply["exceptionDetails"] as
      | { exception?: { description?: string }; text?: string }
      | undefined;
    if (exception)
      throw new Error(`${expression}: ${exception.exception?.description ?? exception.text}`);
    return (reply["result"] as { value: T }).value;
  }

  /** Waits until `expression` is truthy; its value. */
  async waitFor<T>(expression: string, timeoutMs = 10_000): Promise<T> {
    const deadline = Date.now() + timeoutMs;
    let last: unknown;
    for (;;) {
      try {
        const value = await this.eval<T>(expression);
        if (value) return value;
        last = value;
      } catch (error) {
        last = error;
      }
      if (Date.now() > deadline)
        throw new Error(`timed out waiting for ${expression} (last: ${String(last)})`);
      await new Promise((resolve) => setTimeout(resolve, 50));
    }
  }

  async click(selector: string): Promise<void> {
    await this.eval(`document.querySelector(${JSON.stringify(selector)}).click()`);
  }

  /** Chooses `value` in the select labelled `label`. */
  async choose(label: string, value: string): Promise<void> {
    await this.eval(`(() => {
      const select = document.querySelector('select[aria-label=${JSON.stringify(label)}]');
      select.value = ${JSON.stringify(value)};
      select.dispatchEvent(new Event("change"));
    })()`);
  }

  /** A real key press. */
  async press(key: string, code: string, keyCode: number): Promise<void> {
    for (const type of ["keyDown", "keyUp"]) {
      await this.send("Input.dispatchKeyEvent", {
        type,
        key,
        code,
        windowsVirtualKeyCode: keyCode,
      });
    }
  }

  async text(selector: string): Promise<string> {
    return this.eval<string>(
      `document.querySelector(${JSON.stringify(selector)})?.textContent ?? ""`,
    );
  }
}
