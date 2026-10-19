// Routes live in the address's hash, so back, forward and links work.
function decodePath(text) {
    try {
        const path = text.split("/").map(decodeURIComponent).join("/");
        return path === "" ? null : path;
    }
    catch {
        return null;
    }
}
function encodePath(path) {
    return path.split("/").map(encodeURIComponent).join("/");
}
export function parseRoute(hash) {
    const text = hash.replace(/^#/, "");
    const [where = "", query = ""] = text.split("?", 2);
    if (where === "" || where === "/")
        return { page: "home" };
    if (where === "/live")
        return { page: "live" };
    for (const page of ["game", "tournament"]) {
        const prefix = `/${page}/`;
        if (!where.startsWith(prefix))
            continue;
        const path = decodePath(where.slice(prefix.length));
        if (path === null)
            break;
        if (page === "tournament")
            return { page, path };
        const turn = Number(new URLSearchParams(query).get("turn") ?? Number.NaN);
        return { page, path, turn: Number.isSafeInteger(turn) && turn >= 0 ? turn : null };
    }
    return { page: "unknown", hash };
}
export function routeHash(route) {
    switch (route.page) {
        case "home":
            return "#/";
        case "live":
            return "#/live";
        case "tournament":
            return `#/tournament/${encodePath(route.path)}`;
        case "game":
            return `#/game/${encodePath(route.path)}${route.turn === null ? "" : `?turn=${route.turn}`}`;
        case "unknown":
            return route.hash;
    }
}
