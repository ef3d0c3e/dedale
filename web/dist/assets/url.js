/** Builds the URL of an API route, carrying the session token. */
export function apiUrl(path, token, params = {}) {
    const query = new URLSearchParams({ ...params, token });
    return `${path}?${query}`;
}
