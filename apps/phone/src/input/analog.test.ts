import { describe, expect, it } from 'vitest';
import { STICK_DEADZONE, createStickEdge, stickValue, touchPosition } from './analog';

describe('stickValue', () => {
  it('is centred inside the dead zone', () => {
    expect(stickValue(0, 0, 50)).toEqual({ x: 0, y: 0 });
    expect(stickValue(50 * STICK_DEADZONE * 0.9, 0, 50)).toEqual({
      x: 0,
      y: 0,
    });
  });

  it('points up for a thumb above the centre (+y up on the wire, +y down on screen)', () => {
    expect(stickValue(0, -50, 50)).toEqual({ x: 0, y: 1 });
    expect(stickValue(0, 50, 50)).toEqual({ x: 0, y: -1 });
    expect(stickValue(50, 0, 50)).toEqual({ x: 1, y: 0 });
  });

  it('stays inside the unit circle beyond the rim and on diagonals', () => {
    const v = stickValue(200, -200, 50);
    expect(Math.hypot(v.x, v.y)).toBeLessThanOrEqual(1.0015);
    expect(v.x).toBeCloseTo(Math.SQRT1_2, 2);
    expect(v.y).toBeCloseTo(Math.SQRT1_2, 2);
  });

  it('rescales after the dead zone so small moves start near zero', () => {
    const just = stickValue(50 * (STICK_DEADZONE + 0.01), 0, 50);
    expect(just.x).toBeGreaterThan(0);
    expect(just.x).toBeLessThan(0.02);
  });

  it('never returns NaN or -0', () => {
    expect(stickValue(Number.NaN, 0, 50)).toEqual({ x: 0, y: 0 });
    expect(stickValue(10, 10, 0)).toEqual({ x: 0, y: 0 });
    expect(Object.is(stickValue(0, -50, 50).x, -0)).toBe(false);
  });
});

describe('touchPosition', () => {
  const rect = { left: 100, top: 50, width: 400, height: 200 };

  it('normalises to the touch area with the origin at the top-left', () => {
    expect(touchPosition(100, 50, rect)).toEqual({ x: 0, y: 0 });
    expect(touchPosition(300, 150, rect)).toEqual({ x: 0.5, y: 0.5 });
    expect(touchPosition(500, 250, rect)).toEqual({ x: 1, y: 1 });
  });

  it('clamps a finger that slid off the area', () => {
    expect(touchPosition(0, 1000, rect)).toEqual({ x: 0, y: 1 });
  });

  it('returns null for an empty area', () => {
    expect(touchPosition(1, 1, { left: 0, top: 0, width: 0, height: 10 })).toBeNull();
  });
});

describe('stick edge detector', () => {
  const at = (len: number) => ({ x: len, y: 0 });
  const run = (lens: number[]) => {
    const edge = createStickEdge();
    return lens.map((l) => edge.update(at(l)));
  };

  it('fires once when the stick reaches the rim, also when pushed there slowly', () => {
    expect(run([0.5, 0.99, 1])).toEqual([false, true, false]);
    expect(run([0.85, 0.92, 0.95, 0.99])).toEqual([false, false, false, true]);
    const edge = createStickEdge();
    expect(edge.update({ x: 0.71, y: 0.71 })).toBe(true);
  });

  it('needs the stick to come back in before firing again, so a wobbling thumb does not buzz', () => {
    expect(run([1, 0.93, 1, 0.8, 1])).toEqual([true, false, false, false, true]);
  });
});
