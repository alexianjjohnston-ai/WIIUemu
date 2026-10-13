import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  calibrationHapticEvent,
  connectionHapticEvent,
  createHaptics,
  PATTERNS,
  type HapticBackend,
  type HapticStrength,
} from './haptics';

function recorder() {
  const calls: (number | number[])[] = [];
  const backend: HapticBackend = {
    kind: 'vibrate',
    pulse: (pattern) => {
      calls.push(pattern);
    },
  };
  return { calls, backend };
}

describe('haptics', () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it('plays the pattern for the chosen strength', () => {
    const { calls, backend } = recorder();
    let strength: HapticStrength = 'light';
    const h = createHaptics(backend, () => strength);
    h.fire('press');
    strength = 'strong';
    h.fire('press');
    expect(calls).toEqual([PATTERNS.press.light, PATTERNS.press.strong]);
  });

  it('does nothing when switched off or when the phone has no haptics', () => {
    const { calls, backend } = recorder();
    createHaptics(backend, () => 'off').fire('press');
    expect(calls).toEqual([]);
    expect(() => {
      createHaptics(null, () => 'strong').fire('connected');
    }).not.toThrow();
  });

  it('swallows backend errors', () => {
    const backend: HapticBackend = {
      kind: 'vibrate',
      pulse: () => {
        throw new Error('blocked');
      },
    };
    expect(() => {
      createHaptics(backend, () => 'light').fire('press');
    }).not.toThrow();
  });

  it('strong patterns are never weaker than light ones', () => {
    const total = (p: number | number[]) => (Array.isArray(p) ? p.filter((_, i) => i % 2 === 0) : [p]).reduce((a, b) => a + b, 0);
    for (const [event, p] of Object.entries(PATTERNS)) {
      expect(total(p.strong), event).toBeGreaterThanOrEqual(total(p.light));
    }
  });
});

describe('connection haptics', () => {
  const connected = { status: 'connected' } as const;
  const reconnecting = { status: 'reconnecting' } as const;

  it('buzzes when the controller connects and when an open connection drops', () => {
    expect(connectionHapticEvent({ status: 'connecting' }, connected)).toBe('connected');
    expect(connectionHapticEvent(reconnecting, connected)).toBe('connected');
    expect(connectionHapticEvent(connected, reconnecting)).toBe('lost');
    expect(connectionHapticEvent(connected, { status: 'failed' })).toBe('lost');
  });

  it('stays quiet for everything else', () => {
    expect(connectionHapticEvent(connected, connected)).toBeNull();
    expect(connectionHapticEvent({ status: 'connecting' }, reconnecting)).toBeNull();
    expect(connectionHapticEvent(reconnecting, { status: 'failed' })).toBeNull();
    expect(connectionHapticEvent(connected, { status: 'idle' })).toBeNull(); // the player left on purpose
  });
});

describe('calibration haptics', () => {
  it('buzzes once for a new result', () => {
    expect(calibrationHapticEvent({ status: 'measuring' }, { status: 'saved', justNow: true })).toBe('calibrated');
    expect(calibrationHapticEvent({ status: 'measuring' }, { status: 'failed', reason: 'no_markers' })).toBe('failed');
  });

  it('stays quiet for a stored calibration, the countdown and repeated states', () => {
    expect(calibrationHapticEvent({ status: 'none' }, { status: 'saved', justNow: false })).toBeNull();
    expect(calibrationHapticEvent({ status: 'saved', justNow: true }, { status: 'saved', justNow: false })).toBeNull();
    expect(calibrationHapticEvent({ status: 'countdown', seconds: 3 }, { status: 'countdown', seconds: 2 })).toBeNull();
    const failed = { status: 'failed', reason: 'no_markers' } as const;
    expect(calibrationHapticEvent(failed, failed)).toBeNull();
  });
});
