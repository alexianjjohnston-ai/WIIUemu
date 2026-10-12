import { readString, writeString, type KeyValueStore } from './storage';

const KEY = 'wiiu-connect.deviceId';
const VALID = /^[A-Za-z0-9-]{8,64}$/;

/**
 * Random ID. `crypto.randomUUID` only exists in secure contexts (HTTPS / localhost), and the PC
 * server is often reached over plain HTTP on the LAN, so fall back to getRandomValues.
 */
export function generateDeviceId(c: Pick<Crypto, 'getRandomValues'> & Partial<Pick<Crypto, 'randomUUID'>> = crypto): string {
  if (typeof c.randomUUID === 'function') {
    try {
      return c.randomUUID();
    } catch {
      // fall through
    }
  }
  const bytes = c.getRandomValues(new Uint8Array(16));
  return Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('');
}

/** The persistent device ID, created on first use. Falls back to a per-page-load ID without storage. */
export function getDeviceId(store?: KeyValueStore | null): string {
  const existing = readString(KEY, store);
  if (existing !== null && VALID.test(existing)) return existing;
  const id = generateDeviceId();
  writeString(KEY, id, store);
  return id;
}
