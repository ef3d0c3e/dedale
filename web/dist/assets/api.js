// The local server's API: every call carries the token
// from the page's own address.
import { apiUrl } from "./url.js";
import { pace, parseListing, parseLiveEvent, parseReplay, parseTournament, session, } from "./validate.js";
export class ApiError extends Error {
    name = "ApiError";
    status;
    constructor(status, message) {
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
];
/** Reconnects this long after a stream the browser gave up on. */
const RECONNECT_MS = 2000;
export class Api {
    token;
    constructor(token) {
        this.token = token;
    }
    /** The token from `?token=` in the page's address. */
    static fromLocation(location) {
        return new Api(new URLSearchParams(location.search).get("token") ?? "");
    }
    url(path, params = {}) {
        return apiUrl(path, this.token, params);
    }
    async request(method, path, params, body) {
        const init = { method };
        if (body !== undefined) {
            init.headers = { "Content-Type": "application/json" };
            init.body = JSON.stringify(body);
        }
        const response = await fetch(this.url(path, params), init);
        const text = await response.text();
        let data = null;
        try {
            data = text ? JSON.parse(text) : null;
        }
        catch {
            throw new ApiError(response.status, `${path}: the answer is not JSON`);
        }
        if (!response.ok) {
            const message = typeof data === "object" && data !== null && "error" in data
                ? String(data.error)
                : response.statusText;
            throw new ApiError(response.status, message);
        }
        return data;
    }
    async get(path, params = {}) {
        return this.request("GET", path, params);
    }
    async post(path, body) {
        return this.request("POST", path, {}, body);
    }
    async session() {
        return session(await this.get("/api/session"), "session");
    }
    async runs() {
        return parseListing(await this.get("/api/runs"));
    }
    async replay(path) {
        return parseReplay(await this.get("/api/file", { path }));
    }
    async tournament(path) {
        return parseTournament(await this.get("/api/file", { path }));
    }
    async start() {
        await this.post("/api/start");
    }
    async control(change) {
        return pace(await this.post("/api/control", change), "pace");
    }
    /**
     * Follows the live stream until the returned function is called. Every
     * connection starts with a snapshot, so reconnecting catches up.
     */
    events(handlers) {
        let source = null;
        let timer = null;
        let closed = false;
        const connect = () => {
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
                        handlers.event(parseLiveEvent(name, JSON.parse(message.data)));
                    }
                    catch (error) {
                        handlers.invalid(error instanceof Error ? error : new Error(String(error)));
                    }
                });
            }
        };
        connect();
        return () => {
            closed = true;
            if (timer !== null)
                clearTimeout(timer);
            source?.close();
        };
    }
}
