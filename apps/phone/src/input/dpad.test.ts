import { describe, expect, it } from 'vitest';
import { dpadDirections } from './dpad';

const R = 100;
const at = (deg: number, dist = 80) => {
  const rad = (deg * Math.PI) / 180;
  return dpadDirections(Math.cos(rad) * dist, -Math.sin(rad) * dist, R);
};

describe('dpadDirections', () => {
  it('returns nothing in the centre dead zone', () => {
    expect(dpadDirections(0, 0, R)).toEqual([]);
    expect(dpadDirections(10, -10, R)).toEqual([]);
  });

  it('maps the four cardinal directions (screen y grows downwards)', () => {
    expect(dpadDirections(0, -80, R)).toEqual(['UP']);
    expect(dpadDirections(0, 80, R)).toEqual(['DOWN']);
    expect(dpadDirections(-80, 0, R)).toEqual(['LEFT']);
    expect(dpadDirections(80, 0, R)).toEqual(['RIGHT']);
  });

  it('tolerates off-axis touches within the cardinal sector', () => {
    expect(at(90 + 25)).toEqual(['UP']);
    expect(at(-25)).toEqual(['RIGHT']);
  });

  it('produces diagonals near 45 degrees', () => {
    expect(at(45)).toEqual(['UP', 'RIGHT']);
    expect(at(135)).toEqual(['UP', 'LEFT']);
    expect(at(225)).toEqual(['DOWN', 'LEFT']);
    expect(at(315)).toEqual(['DOWN', 'RIGHT']);
  });

  it('keeps registering when the finger slides beyond the pad (pointer capture)', () => {
    expect(dpadDirections(0, -500, R)).toEqual(['UP']);
  });

  it('rejects invalid input', () => {
    expect(dpadDirections(NaN, 0, R)).toEqual([]);
    expect(dpadDirections(10, 0, 0)).toEqual([]);
  });
});
