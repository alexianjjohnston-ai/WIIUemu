// Pure geometry for sticks and the touchscreen. No DOM, unit-tested.

import type { Vec2 } from '../protocol/types';

/** Stick travel below this fraction of the radius reads as centred (thumb jitter). */
export const STICK_DEADZONE = 0.08;

const round = (v: number, places: number) => {
  const f = 10 ** places;
  const r = Math.round(v * f) / f;
  return r === 0 ? 0 : r; // no -0 on the wire
};

/**
 * Thumb offset from the stick centre (screen pixels, +y down) → protocol stick value (+y up), limited to the
 * unit circle. The dead zone is radial and rescaled, so output still starts at 0 and reaches 1 at the rim.
 */
export function stickValue(dx: number, dy: number, radius: number, deadzone = STICK_DEADZONE): Vec2 {
  if (!(radius > 0) || !Number.isFinite(dx) || !Number.isFinite(dy)) return { x: 0, y: 0 };
  const nx = dx / radius;
  const ny = -dy / radius;
  const len = Math.hypot(nx, ny);
  if (len <= deadzone) return { x: 0, y: 0 };
  const scaled = Math.min(1, (len - deadzone) / (1 - deadzone));
  const x = round((nx / len) * scaled, 3);
  const y = round((ny / len) * scaled, 3);
  return { x: Math.max(-1, Math.min(1, x)), y: Math.max(-1, Math.min(1, y)) };
}

export interface RectLike {
  left: number;
  top: number;
  width: number;
  height: number;
}

/** A point on the touch area → normalised `[0, 1]` position (origin top-left), clamped to the area. */
export function touchPosition(clientX: number, clientY: number, rect: RectLike): Vec2 | null {
  if (!(rect.width > 0) || !(rect.height > 0) || !Number.isFinite(clientX) || !Number.isFinite(clientY)) return null;
  const clamp = (v: number) => Math.max(0, Math.min(1, v));
  return {
    x: round(clamp((clientX - rect.left) / rect.width), 4),
    y: round(clamp((clientY - rect.top) / rect.height), 4),
  };
}

/** Stick length at which the rim counts as reached, and below which the next rim hit counts again. */
const EDGE_ON = 0.97;
const EDGE_REARM = 0.9;

/** Reports when a stick reaches its rim; it fires again only after the stick has come back in. */
export function createStickEdge(): { update(v: Vec2): boolean } {
  let armed = true;
  return {
    update(v) {
      const len = Math.hypot(v.x, v.y);
      if (armed && len >= EDGE_ON) {
        armed = false;
        return true;
      }
      if (len < EDGE_REARM) armed = true;
      return false;
    },
  };
}
