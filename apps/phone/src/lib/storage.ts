// localStorage can be missing or throw (private mode, blocked storage, sandboxed iframes).
// Every access goes through these helpers so the app keeps working without persistence.

export interface KeyValueStore {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

function defaultStore(): KeyValueStore | null {
  try {
    return typeof localStorage === 'undefined' ? null : localStorage;
  } catch {
    return null;
  }
}

export function readString(key: string, store: KeyValueStore | null = defaultStore()): string | null {
  try {
    return store?.getItem(key) ?? null;
  } catch {
    return null;
  }
}

export function writeString(key: string, value: string, store: KeyValueStore | null = defaultStore()): void {
  try {
    store?.setItem(key, value);
  } catch {
    // Quota exceeded or storage disabled: persistence is best-effort.
  }
}

export function readJson(key: string, store?: KeyValueStore | null): unknown {
  const raw = readString(key, store);
  if (raw === null) return null;
  try {
    return JSON.parse(raw) as unknown;
  } catch {
    return null;
  }
}

export function writeJson(key: string, value: unknown, store?: KeyValueStore | null): void {
  writeString(key, JSON.stringify(value), store);
}
