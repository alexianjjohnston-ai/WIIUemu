import { describe, expect, it, vi } from 'vitest';
import { validateInput } from '../protocol/validate';
import {
  STANDARD_GRAVITY as G,
  detectConvention,
  deviceGravity,
  deviceRates,
  readingToMotion,
  toControllerFrame,
  toDsuMotion,
  toScreenFrame,
  type MotionReading,
} from './convert';
import { MotionSensor, currentScreenAngle } from './MotionSensor';

/** A W3C (Chromium) reading. */
function w3c(
  accel: [number, number, number],
  rates: { alpha: number; beta: number; gamma: number },
  t = 10,
): MotionReading {
  return {
    accelerationIncludingGravity: { x: accel[0], y: accel[1], z: accel[2] },
    rotationRate: rates,
    timeStamp: t,
  };
}

const still = { alpha: 0, beta: 0, gamma: 0 };
/** -0 → 0, so frame rotations compare by value. */
const n = (v: number[]) => v.map((x) => x + 0);

describe('engine conventions (source-verified, docs/research.md §9)', () => {
  it('detects WebKit by DeviceMotionEvent.requestPermission or an iOS user agent', () => {
    expect(detectConvention({ requestPermission: () => Promise.resolve('granted') }, '')).toBe('webkit-ios');
    expect(detectConvention(function DeviceMotionEvent() {}, 'Mozilla/5.0 (iPhone; CPU iPhone OS 18_0)')).toBe(
      'webkit-ios',
    );
    expect(detectConvention(function DeviceMotionEvent() {}, 'Mozilla/5.0 (Linux; Android 15; Pixel 7)')).toBe('w3c');
    expect(detectConvention(undefined, '')).toBe('w3c');
  });

  it('maps rotationRate to device x/y/z per engine', () => {
    const r = { alpha: 1, beta: 2, gamma: 3 };
    expect(deviceRates(r, 'w3c')).toEqual([2, 3, 1]); // beta = x, gamma = y, alpha = z
    expect(deviceRates(r, 'webkit-ios')).toEqual([1, 2, 3]); // CoreMotion x, y, z passed straight through
    expect(deviceRates({ alpha: null, beta: 2, gamma: 3 }, 'w3c')).toBeNull();
    expect(deviceRates({ alpha: Number.NaN, beta: 2, gamma: 3 }, 'w3c')).toBeNull();
  });

  it('WebKit reports gravity pointing down; it is flipped to the W3C sign', () => {
    expect(n(deviceGravity({ x: 0, y: 0, z: -G }, 'webkit-ios') ?? [])).toEqual([0, 0, G]);
    expect(deviceGravity({ x: 0, y: 0, z: G }, 'w3c')).toEqual([0, 0, G]);
    expect(deviceGravity(null, 'w3c')).toBeNull();
  });
});

describe('frames', () => {
  it('rotates the device frame into the screen frame for every orientation', () => {
    const top: [number, number, number] = [0, 1, 0]; // the phone's top edge
    expect(n(toScreenFrame(top, 0))).toEqual([0, 1, 0]);
    // Landscape, top edge to the left: the top edge is the screen's -x.
    expect(n(toScreenFrame(top, 90))).toEqual([-1, 0, 0]);
    expect(n(toScreenFrame(top, 180))).toEqual([0, -1, 0]);
    expect(n(toScreenFrame(top, 270))).toEqual([1, 0, 0]);
  });

  it('face grip: the screen is the controller face, the top edge points away from the player', () => {
    // Screen normal → controller +y (up out of the face); screen top → controller -z (forward).
    expect(n(toControllerFrame([0, 0, 1], 'face'))).toEqual([0, 1, 0]);
    expect(n(toControllerFrame([0, 1, 0], 'face'))).toEqual([0, 0, -1]);
    expect(n(toControllerFrame([1, 0, 0], 'face'))).toEqual([1, 0, 0]);
  });

  it('upright grip keeps the screen frame (back of the phone faces the TV)', () => {
    expect(toControllerFrame([1, 2, 3], 'upright')).toEqual([1, 2, 3]);
  });
});

describe('DSU motion (Cemu convention)', () => {
  it('a controller lying flat reads accel (0, -1, 0), like the protocol example', () => {
    expect(toDsuMotion([0, G, 0], [0, 0, 0], 5).accel).toEqual([0, -1, 0]);
  });

  it('negates gyro y and z, keeps x', () => {
    expect(toDsuMotion([0, 0, 0], [10, 20, 30], 0).gyro).toEqual([10, -20, -30]);
  });

  it('clamps saturated sensors to the protocol limits and uses integer µs timestamps', () => {
    const m = toDsuMotion([1e6, 0, 0], [1e5, 0, 0], 1234.9);
    expect(m.accel[0]).toBe(-64);
    expect(m.gyro[0]).toBe(8000);
    expect(m.ts).toBe(1234);
  });

  it('GamePad flat on a table in landscape: same sample on Chromium and on iOS', () => {
    const android = readingToMotion(w3c([0, 0, G], { alpha: 30, beta: 0, gamma: 0 }), 'w3c', 90, 'face');
    const ios = readingToMotion(
      {
        accelerationIncludingGravity: { x: 0, y: 0, z: -G },
        rotationRate: { alpha: 0, beta: 0, gamma: 30 },
        timeStamp: 10,
      },
      'webkit-ios',
      90,
      'face',
    );
    expect(android).toEqual({
      accel: [0, -1, 0],
      gyro: [0, -30, 0],
      ts: 10_000,
    });
    expect(ios).toEqual(android);
  });

  it('turning a landscape GamePad left (yaw) shows up on the controller yaw axis only', () => {
    // Held flat, rotating counter-clockwise seen from above = rotation about device z.
    const m = readingToMotion(w3c([0, 0, G], { alpha: 45, beta: 0, gamma: 0 }), 'w3c', 90, 'face');
    expect(m?.gyro).toEqual([0, -45, 0]);
  });

  it('tilting a landscape GamePad away (pitch) uses the screen x axis', () => {
    // Landscape (angle 90): the screen's x axis is the phone's -y axis, so pitch is a rotation about device y.
    const m = readingToMotion(w3c([0, 0, G], { alpha: 0, beta: 0, gamma: -20 }), 'w3c', 90, 'face');
    expect(m?.gyro).toEqual([20, 0, 0]);
  });

  it('every converted sample passes the protocol validator', () => {
    const m = readingToMotion(w3c([3, -4, 9], { alpha: 400, beta: -250, gamma: 90 }, 123.456), 'w3c', 270, 'face');
    expect(m).not.toBeNull();
    expect(validateInput({ type: 'input', seq: 1, buttons: [], pointer: null, motion: m }, 'gamepad')).toEqual([]);
  });

  it('skips readings without gyro or accel', () => {
    expect(
      readingToMotion(
        {
          accelerationIncludingGravity: null,
          rotationRate: still,
          timeStamp: 1,
        },
        'w3c',
        0,
        'face',
      ),
    ).toBeNull();
    expect(
      readingToMotion(
        {
          accelerationIncludingGravity: { x: 0, y: 0, z: 0 },
          rotationRate: null,
          timeStamp: 1,
        },
        'w3c',
        0,
        'face',
      ),
    ).toBeNull();
  });
});

describe('MotionSensor', () => {
  function fakeTarget() {
    const handlers = new Set<(e: unknown) => void>();
    return {
      addEventListener: vi.fn((_: string, h: (e: unknown) => void) => handlers.add(h)),
      removeEventListener: vi.fn((_: string, h: (e: unknown) => void) => handlers.delete(h)),
      fire: (e: MotionReading) => {
        for (const h of handlers) h(e);
      },
      count: () => handlers.size,
    };
  }

  it('listens only while on and reports the latest converted sample', async () => {
    const target = fakeTarget();
    const onSample = vi.fn();
    const sensor = new MotionSensor('face', {
      target,
      onSample,
      requestPermission: () => Promise.resolve(true),
      screenAngle: () => 90,
      convention: 'w3c',
      supported: true,
    });
    expect(sensor.getLatest()).toBeNull();
    await sensor.enable();
    expect(sensor.getStatus()).toBe('on');
    target.fire(w3c([0, 0, G], { alpha: 45, beta: 0, gamma: 0 }, 20));
    expect(onSample).toHaveBeenCalledTimes(1);
    expect(sensor.getLatest()).toEqual({
      accel: [0, -1, 0],
      gyro: [0, -45, 0],
      ts: 20_000,
    });
    sensor.disable();
    expect(target.count()).toBe(0);
    expect(sensor.getLatest()).toBeNull();
  });

  it('reports denied permission and does not listen', async () => {
    const target = fakeTarget();
    const sensor = new MotionSensor('face', {
      target,
      requestPermission: () => Promise.resolve(false),
      supported: true,
    });
    await sensor.enable();
    expect(sensor.getStatus()).toBe('denied');
    expect(target.count()).toBe(0);
  });

  it('reports unsupported browsers', async () => {
    const sensor = new MotionSensor('face', {
      target: fakeTarget(),
      supported: false,
    });
    await sensor.enable();
    expect(sensor.getStatus()).toBe('unsupported');
  });

  it('normalises the screen angle', () => {
    expect([0, 90, 180, 270]).toContain(currentScreenAngle());
  });
});
