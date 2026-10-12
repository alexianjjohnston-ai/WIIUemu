// URL handling: `?token=` (pairing token from the PC's QR code) and `?server=` (override honoured only by the
// Vite dev server, see App.tsx).

export interface LaunchParams {
  token: string | null;
  server: string | null;
}

export function readLaunchParams(search: string): LaunchParams {
  const q = new URLSearchParams(search);
  const token = q.get('token')?.trim() ?? '';
  const server = q.get('server')?.trim() ?? '';
  return { token: token || null, server: server || null };
}

/**
 * WebSocket URL for the controller endpoint.
 * - no override: same host that served the page, `/ws`, `wss:` when the page is HTTPS
 * - `ws://…` / `wss://…`: used as is
 * - `http(s)://host[:port][/path]`: scheme swapped, `/ws` appended when no path is given
 * - `host[:port]`: scheme chosen like the default, `/ws` appended
 */
export function resolveWsUrl(location: Pick<Location, 'protocol' | 'host'>, server: string | null): string {
  const secure = location.protocol === 'https:';
  const defaultScheme = secure ? 'wss:' : 'ws:';
  if (!server) return `${defaultScheme}//${location.host}/ws`;

  let candidate = server;
  if (!/^[a-z]+:\/\//i.test(candidate)) candidate = `${defaultScheme}//${candidate}`;
  try {
    const url = new URL(candidate);
    if (url.protocol === 'http:') url.protocol = 'ws:';
    if (url.protocol === 'https:') url.protocol = 'wss:';
    if (url.protocol !== 'ws:' && url.protocol !== 'wss:') return `${defaultScheme}//${location.host}/ws`;
    if (url.pathname === '' || url.pathname === '/') url.pathname = '/ws';
    return url.toString();
  } catch {
    return `${defaultScheme}//${location.host}/ws`;
  }
}

/** A short, non-identifying label for the PC diagnostics UI (protocol: at most 40 chars). */
export function guessDeviceName(userAgent: string): string {
  if (/iPhone/.test(userAgent)) return 'iPhone';
  if (/iPad/.test(userAgent)) return 'iPad';
  if (/Android/.test(userAgent)) return 'Android phone';
  return 'Phone';
}
