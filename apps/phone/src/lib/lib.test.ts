import { describe, expect, it } from 'vitest';
import { guessDeviceName, readLaunchParams, resolveWsUrl } from './params';
import { clampScale, DEFAULT_SETTINGS, loadSettings, parseSettings, saveSettings } from './settings';
import { generateDeviceId, getDeviceId } from './deviceId';
import { readString, writeString, type KeyValueStore } from './storage';

function memoryStore(): KeyValueStore & { data: Map<string, string> } {
  const data = new Map<string, string>();
  return {
    data,
    getItem: (k) => data.get(k) ?? null,
    setItem: (k, v) => {
      data.set(k, v);
    },
  };
}

const throwingStore: KeyValueStore = {
  getItem: () => {
    throw new Error('SecurityError');
  },
  setItem: () => {
    throw new Error('QuotaExceededError');
  },
};

describe('launch params', () => {
  it('reads token and server from the query string', () => {
    expect(readLaunchParams('?token=abc123&server=10.0.0.2:8080')).toEqual({ token: 'abc123', server: '10.0.0.2:8080' });
    expect(readLaunchParams('')).toEqual({ token: null, server: null });
    expect(readLaunchParams('?token=%20%20')).toEqual({ token: null, server: null });
  });

  it('builds the default ws URL from the page location', () => {
    expect(resolveWsUrl({ protocol: 'http:', host: '192.168.1.5:8787' }, null)).toBe('ws://192.168.1.5:8787/ws');
    expect(resolveWsUrl({ protocol: 'https:', host: 'pc.local' }, null)).toBe('wss://pc.local/ws');
  });

  it('accepts several server override forms', () => {
    const loc = { protocol: 'http:', host: 'localhost:5173' };
    expect(resolveWsUrl(loc, 'ws://10.0.0.2:8787/ws')).toBe('ws://10.0.0.2:8787/ws');
    expect(resolveWsUrl(loc, '10.0.0.2:8787')).toBe('ws://10.0.0.2:8787/ws');
    expect(resolveWsUrl(loc, 'https://pc.example')).toBe('wss://pc.example/ws');
    expect(resolveWsUrl(loc, 'ftp://nope')).toBe('ws://localhost:5173/ws');
  });

  it('guesses a short device name', () => {
    expect(guessDeviceName('Mozilla/5.0 (Linux; Android 14; Pixel 7)')).toBe('Android phone');
    expect(guessDeviceName('Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X)')).toBe('iPhone');
  });
});

describe('settings', () => {
  it('falls back to defaults for missing or invalid data', () => {
    expect(parseSettings(null)).toEqual(DEFAULT_SETTINGS);
    expect(parseSettings({ layout: 'huge', leftHanded: 'yes', scale: 'big' })).toEqual(DEFAULT_SETTINGS);
  });

  it('clamps the scale to 0.8-1.4', () => {
    expect(clampScale(3)).toBe(1.4);
    expect(clampScale(0.1)).toBe(0.8);
    expect(clampScale(Number.NaN)).toBe(1);
    expect(parseSettings({ layout: 'comfort', leftHanded: true, scale: 1.234 })).toEqual({
      layout: 'comfort',
      leftHanded: true,
      scale: 1.23,
    });
  });

  it('round-trips through storage', () => {
    const store = memoryStore();
    saveSettings({ layout: 'comfort', leftHanded: true, scale: 1.2 }, store);
    expect(loadSettings(store)).toEqual({ layout: 'comfort', leftHanded: true, scale: 1.2 });
  });

  it('survives storage that throws', () => {
    expect(() => {
      saveSettings(DEFAULT_SETTINGS, throwingStore);
    }).not.toThrow();
    expect(loadSettings(throwingStore)).toEqual(DEFAULT_SETTINGS);
  });
});

describe('device id', () => {
  const ID_RE = /^[A-Za-z0-9-]{8,64}$/;

  it('generates protocol-valid IDs with and without randomUUID', () => {
    expect(generateDeviceId()).toMatch(ID_RE);
    const insecure = { getRandomValues: crypto.getRandomValues.bind(crypto) } as Pick<Crypto, 'getRandomValues'>;
    expect(generateDeviceId(insecure)).toMatch(ID_RE);
  });

  it('persists the ID and replaces an invalid stored value', () => {
    const store = memoryStore();
    const id = getDeviceId(store);
    expect(getDeviceId(store)).toBe(id);
    store.data.set('wiiu-connect.deviceId', 'bad id!');
    expect(getDeviceId(store)).not.toBe('bad id!');
  });

  it('still returns an ID when storage throws', () => {
    expect(getDeviceId(throwingStore)).toMatch(ID_RE);
  });
});

describe('storage helpers', () => {
  it('swallow errors', () => {
    expect(readString('x', throwingStore)).toBeNull();
    expect(() => {
      writeString('x', 'y', throwingStore);
    }).not.toThrow();
    expect(readString('x', null)).toBeNull();
  });
});
