// DeviceMotionEvent → protocol `motion` (DSU axis convention). Pure functions, unit-tested.
//
// Browsers disagree on DeviceMotionEvent axes (source-verified, see docs/research.md §9):
// - Chromium / Firefox follow the W3C spec: `rotationRate` beta/gamma/alpha are the rates about device x/y/z, and
//   `accelerationIncludingGravity` points *up* at rest (lying flat, screen up: z = +9.81).
// - WebKit on iOS passes CoreMotion through: `rotationRate` alpha/beta/gamma are the rates about x/y/z, and
//   `accelerationIncludingGravity` points *down* (z = -9.81 lying flat).
//
// Both are first brought into the W3C device frame (x right, y toward the top edge, z out of the screen), then
// into the controller frame, then into the DSU convention Cemu expects.

import { MAX_MOTION_ACCEL_G, MAX_MOTION_GYRO_DPS, type Motion } from '../protocol/types';

export type Vec3 = [number, number, number];

export type MotionConvention = 'w3c' | 'webkit-ios';

/**
 * How the phone is held relative to the emulated controller:
 * - `face`: the screen is the controller's face and the top edge points forward (GamePad held in landscape, or a
 *   phone held like a Wii Remote, top edge toward the TV).
 * - `upright`: the back of the phone faces the TV, top edge up ("photo grip" for camera pointing).
 */
export type MotionGrip = 'face' | 'upright';

export interface AccelLike {
  x: number | null;
  y: number | null;
  z: number | null;
}

export interface RotationRateLike {
  alpha: number | null;
  beta: number | null;
  gamma: number | null;
}

export const STANDARD_GRAVITY = 9.80665;

interface MotionPermissionApi {
  requestPermission?: () => Promise<string>;
}

/** WebKit (every iOS browser) is the only engine with `DeviceMotionEvent.requestPermission`. */
export function detectConvention(
  motionEvent: unknown = typeof DeviceMotionEvent === 'undefined' ? undefined : DeviceMotionEvent,
  userAgent: string = typeof navigator === 'undefined' ? '' : navigator.userAgent,
): MotionConvention {
  const api = motionEvent as MotionPermissionApi | undefined;
  if (typeof api?.requestPermission === 'function') return 'webkit-ios';
  return /iPhone|iPad|iPod/.test(userAgent) ? 'webkit-ios' : 'w3c';
}

/** The three values as a vector, or null if any is missing or not finite. */
function triple(a: number | null, b: number | null, c: number | null): Vec3 | null {
  if (a === null || b === null || c === null) return null;
  return Number.isFinite(a) && Number.isFinite(b) && Number.isFinite(c) ? [a, b, c] : null;
}

/** Rotation rate about device x, y, z in deg/s (right-handed, W3C device frame). */
export function deviceRates(r: RotationRateLike | null | undefined, convention: MotionConvention): Vec3 | null {
  if (!r) return null;
  // `v` holds alpha, beta, gamma in that order.
  const v = triple(r.alpha, r.beta, r.gamma);
  if (!v) return null;
  return convention === 'webkit-ios' ? v : [v[1], v[2], v[0]];
}

/** Acceleration including gravity in m/s², W3C sign (points up at rest). */
export function deviceGravity(a: AccelLike | null | undefined, convention: MotionConvention): Vec3 | null {
  if (!a) return null;
  const v = triple(a.x, a.y, a.z);
  if (!v) return null;
  return convention === 'webkit-ios' ? [-v[0], -v[1], -v[2]] : v;
}

/**
 * Device frame → screen frame. `angle` is `screen.orientation.angle` (degrees the content is rotated
 * counter-clockwise from the natural orientation); in landscape the screen's x is the phone's long side.
 */
export function toScreenFrame(v: Vec3, angle: number): Vec3 {
  const rad = (angle * Math.PI) / 180;
  const c = Math.round(Math.cos(rad));
  const s = Math.round(Math.sin(rad));
  return [v[0] * c - v[1] * s, v[0] * s + v[1] * c, v[2]];
}

/** Screen frame → controller frame (SDL convention: x right, y out of the face, z toward the player). */
export function toControllerFrame(v: Vec3, grip: MotionGrip): Vec3 {
  return grip === 'face' ? [v[0], v[2], -v[1]] : [v[0], v[1], v[2]];
}

const clamp = (v: number, max: number) => Math.max(-max, Math.min(max, v));
const round = (v: number, places: number) => {
  const f = 10 ** places;
  const r = Math.round(v * f) / f;
  return r === 0 ? 0 : r;
};

/**
 * Controller-frame acceleration (m/s², points up) and rotation rate (deg/s) → protocol `motion`.
 *
 * Cemu feeds DSU motion and SDL controller motion into the same motion handler (DSUControllerProvider.cpp
 * `integrate_motion`, SDLControllerProvider.cpp). Equating the two gives DSU accel = -SDL accel / g and DSU gyro =
 * (x, -y, -z). Values are clamped to the protocol limits: a phone that saturates is still in range.
 */
export function toDsuMotion(accel: Vec3, rates: Vec3, tsUs: number): Motion {
  const a = accel.map((v) => round(clamp(-v / STANDARD_GRAVITY, MAX_MOTION_ACCEL_G), 4)) as Vec3;
  const g: Vec3 = [rates[0], -rates[1], -rates[2]].map((v) => round(clamp(v, MAX_MOTION_GYRO_DPS), 3)) as Vec3;
  return { accel: a, gyro: g, ts: Math.max(0, Math.floor(tsUs)) };
}

export interface MotionReading {
  accelerationIncludingGravity: AccelLike | null;
  rotationRate: RotationRateLike | null;
  /** ms, `performance.now()` clock. */
  timeStamp: number;
}

/** One DeviceMotionEvent → protocol `motion`, or null when the browser gave no usable values. */
export function readingToMotion(
  e: MotionReading,
  convention: MotionConvention,
  screenAngle: number,
  grip: MotionGrip,
): Motion | null {
  const accel = deviceGravity(e.accelerationIncludingGravity, convention);
  const rates = deviceRates(e.rotationRate, convention);
  if (!accel || !rates || !Number.isFinite(e.timeStamp)) return null;
  const frame = (v: Vec3) => toControllerFrame(toScreenFrame(v, screenAngle), grip);
  return toDsuMotion(frame(accel), frame(rates), e.timeStamp * 1000);
}
