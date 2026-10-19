/** Builds the URL of an API route, carrying the session token. */
export function apiUrl(path: string, token: string, params: Record<string, string> = {}): string {
  const query = new URLSearchParams({ ...params, token });
  return `${path}?${query}`;
}
