export interface BackoffOptions {
  baseMs: number;
  maxMs: number;
  /** Fraction of random spread, e.g. 0.2 => delay * [0.8, 1.2). */
  jitter: number;
}

export const DEFAULT_BACKOFF: BackoffOptions = { baseMs: 500, maxMs: 8000, jitter: 0.2 };

/**
 * Exponential backoff with jitter. `attempt` is the number of consecutive failures (1 = first retry).
 * `random` is injectable for tests.
 */
export function backoffDelay(
  attempt: number,
  options: BackoffOptions = DEFAULT_BACKOFF,
  random: () => number = Math.random,
): number {
  const exp = Math.min(options.maxMs, options.baseMs * 2 ** Math.max(0, attempt - 1));
  const spread = 1 - options.jitter + 2 * options.jitter * random();
  return Math.round(Math.min(options.maxMs * (1 + options.jitter), exp * spread));
}
