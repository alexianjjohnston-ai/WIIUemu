// Haptic feedback. Android browsers have the Vibration API. iOS Safari has none, but from iOS 18 it plays a
// system haptic when a `<input type="checkbox" switch>` toggles, also when the toggle comes from script, so the
// iOS backend clicks a hidden switch. That trick is unverified on a real iPhone (docs/compatibility.md).

import type { ConnectionState } from '../net/connection';
import type { CalibrationStatus } from '../tracking/CameraPointer';

export type HapticStrength = 'off' | 'light' | 'strong';

export type HapticEvent =
  /** A button went down. */
  | 'press'
  /** The D-pad gained a direction. */
  | 'dpad'
  /** A stick reached its rim. */
  | 'stickEdge'
  /** The controller connected (also after a reconnect). */
  | 'connected'
  /** An open connection dropped. */
  | 'lost'
  /** Pointer calibration saved. */
  | 'calibrated'
  /** Pointer calibration failed. */
  | 'failed';

/** Vibration API patterns (ms on, off, on, …). The iOS backend plays one tick per "on" part. */
export const PATTERNS: Record<HapticEvent, { light: number | number[]; strong: number | number[] }> = {
  press: { light: 8, strong: 18 },
  dpad: { light: 8, strong: 18 },
  stickEdge: { light: 6, strong: 14 },
  connected: { light: [12, 80, 12], strong: [25, 80, 25] },
  lost: { light: [30, 60, 30, 60, 30], strong: [60, 60, 60, 60, 60] },
  calibrated: { light: [12, 80, 12], strong: [25, 80, 25] },
  failed: { light: 60, strong: 120 },
};

export interface HapticBackend {
  readonly kind: 'vibrate' | 'ios-switch';
  pulse(pattern: number | number[]): void;
}

export interface Haptics {
  fire(event: HapticEvent): void;
}

export function createHaptics(backend: HapticBackend | null, strength: () => HapticStrength): Haptics {
  return {
    fire(event) {
      const s = strength();
      if (!backend || s === 'off') return;
      try {
        backend.pulse(PATTERNS[event][s]);
      } catch {
        // Some browsers throw when vibration is blocked (e.g. before the first tap).
      }
    },
  };
}

/** The connection change worth a buzz, if any. Leaving on purpose (`idle`) is silent. */
export function connectionHapticEvent(
  prev: Pick<ConnectionState, 'status'>,
  next: Pick<ConnectionState, 'status'>,
): HapticEvent | null {
  if (next.status === 'connected' && prev.status !== 'connected') return 'connected';
  if (prev.status === 'connected' && (next.status === 'reconnecting' || next.status === 'failed')) return 'lost';
  return null;
}

/** A fresh calibration result worth a buzz, if any. */
export function calibrationHapticEvent(prev: CalibrationStatus, next: CalibrationStatus): HapticEvent | null {
  if (next.status === 'saved' && next.justNow && !(prev.status === 'saved' && prev.justNow)) return 'calibrated';
  if (next.status === 'failed' && prev.status !== 'failed') return 'failed';
  return null;
}

function vibrateBackend(): HapticBackend | null {
  if (typeof navigator === 'undefined' || typeof navigator.vibrate !== 'function') return null;
  return { kind: 'vibrate', pulse: (pattern) => navigator.vibrate(pattern) };
}

function iosSwitchBackend(): HapticBackend | null {
  if (typeof document === 'undefined') return null;
  const input = document.createElement('input');
  // Safari 17.4+ knows the `switch` attribute; other browsers have no such property.
  if (!('switch' in input)) return null;
  input.type = 'checkbox';
  input.setAttribute('switch', '');
  input.tabIndex = -1;
  const label = document.createElement('label');
  label.setAttribute('aria-hidden', 'true');
  label.style.display = 'none';
  label.appendChild(input);
  let attached = false;
  const tick = () => {
    if (!attached) {
      document.body.appendChild(label);
      attached = true;
    }
    label.click();
  };
  return {
    kind: 'ios-switch',
    pulse(pattern) {
      const parts = Array.isArray(pattern) ? pattern : [pattern];
      let at = 0;
      parts.forEach((ms, i) => {
        if (i % 2 === 0) {
          if (at === 0) tick();
          else setTimeout(tick, at);
        }
        at += ms;
      });
    },
  };
}

let strength: HapticStrength = 'light';

/** Follows the player's setting (`useSettings` keeps it current). */
export function setHapticStrength(s: HapticStrength): void {
  strength = s;
}

const haptics = createHaptics(vibrateBackend() ?? iosSwitchBackend(), () => strength);

export function haptic(event: HapticEvent): void {
  haptics.fire(event);
}
