import { readString, writeString } from './storage';

// Remember the last pairing token so an installed PWA (whose start_url has no ?token=)
// can reconnect without rescanning the QR code.
const KEY = 'wiiu-connect.lastToken';

export function loadLastToken(): string | null {
  return readString(KEY);
}

export function saveLastToken(token: string): void {
  writeString(KEY, token);
}
