import type { HapticStrength } from './haptics';
import { readJson, writeJson, type KeyValueStore } from './storage';

export type LayoutName = 'authentic' | 'comfort';

export interface Settings {
  layout: LayoutName;
  /** Mirror the B zone to the left edge. */
  leftHanded: boolean;
  /** Button size multiplier. */
  scale: number;
  /** Buzz on presses and events (shared by the Wii Remote and the GamePad). */
  haptics: HapticStrength;
}

export const SCALE_MIN = 0.8;
export const SCALE_MAX = 1.4;
export const SCALE_STEP = 0.05;

export const DEFAULT_SETTINGS: Settings = { layout: 'authentic', leftHanded: false, scale: 1, haptics: 'light' };

const HAPTIC_STRENGTHS: readonly HapticStrength[] = ['off', 'light', 'strong'];

const KEY = 'wiiu-connect.settings.v1';

export function clampScale(value: number): number {
  if (!Number.isFinite(value)) return DEFAULT_SETTINGS.scale;
  const clamped = Math.min(SCALE_MAX, Math.max(SCALE_MIN, value));
  return Math.round(clamped * 100) / 100;
}

/** Validate untrusted stored data, falling back to defaults field by field. */
export function parseSettings(raw: unknown): Settings {
  if (typeof raw !== 'object' || raw === null) return { ...DEFAULT_SETTINGS };
  const r = raw as Record<string, unknown>;
  return {
    layout: r.layout === 'comfort' || r.layout === 'authentic' ? r.layout : DEFAULT_SETTINGS.layout,
    leftHanded: typeof r.leftHanded === 'boolean' ? r.leftHanded : DEFAULT_SETTINGS.leftHanded,
    scale: typeof r.scale === 'number' ? clampScale(r.scale) : DEFAULT_SETTINGS.scale,
    haptics: HAPTIC_STRENGTHS.find((h) => h === r.haptics) ?? DEFAULT_SETTINGS.haptics,
  };
}

export function loadSettings(store?: KeyValueStore | null): Settings {
  return parseSettings(readJson(KEY, store));
}

export function saveSettings(settings: Settings, store?: KeyValueStore | null): void {
  writeJson(KEY, settings, store);
}
