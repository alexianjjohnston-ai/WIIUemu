// Protocol 2 `gyro` streaming: DeviceMotionEvent.rotationRate → batched rad/s samples. Pure, unit-tested.

import { deviceRates, type MotionConvention, type RotationRateLike } from '../motion/convert';
import { MAX_GYRO_RAD_S, MAX_GYRO_SAMPLES, type GyroMessage } from '../protocol/types';

export type { RotationRateLike };

const DEG = Math.PI / 180;

/**
 * rotationRate (deg/s) → `[x, y, z]` rad/s in the W3C device frame. The axis order depends on the browser engine
 * (see `deviceRates`). Null when a value is missing or outside the protocol limit (skipped, never clamped).
 */
export function rotationRateToRad(
  r: RotationRateLike | null | undefined,
  convention: MotionConvention = 'w3c',
): [number, number, number] | null {
  const deg = deviceRates(r, convention);
  if (!deg) return null;
  const rad: [number, number, number] = [deg[0] * DEG, deg[1] * DEG, deg[2] * DEG];
  return rad.every((v) => Math.abs(v) <= MAX_GYRO_RAD_S) ? rad : null;
}

export interface GyroBatcherOptions {
  /** Send once the oldest buffered sample is this old (µs). Small, because the PC replays gyro after late fixes. */
  maxAgeUs?: number;
  maxSamples?: number;
}

/**
 * Buffers samples and emits `gyro` messages. Timestamps are integer µs and strictly increase across messages;
 * duplicates and out-of-order samples are dropped. A message that can't be sent is discarded: stale gyro is useless.
 */
export class GyroBatcher {
  private samples: [number, number, number, number][] = [];
  private lastT: number | null = null;
  private readonly maxAgeUs: number;
  private readonly maxSamples: number;

  constructor(
    private readonly send: (msg: GyroMessage) => boolean,
    opts: GyroBatcherOptions = {},
  ) {
    this.maxAgeUs = opts.maxAgeUs ?? 16_000;
    this.maxSamples = Math.min(MAX_GYRO_SAMPLES, opts.maxSamples ?? 32);
  }

  add(tUs: number, rate: [number, number, number]): void {
    const t = Math.floor(tUs);
    if (!Number.isSafeInteger(t) || t < 0 || (this.lastT !== null && t <= this.lastT)) return;
    this.lastT = t;
    const round = (v: number) => Math.round(v * 1e5) / 1e5;
    this.samples.push([t, round(rate[0]), round(rate[1]), round(rate[2])]);
    const first = this.samples[0];
    if (this.samples.length >= this.maxSamples || (first !== undefined && t - first[0] >= this.maxAgeUs)) this.flush();
  }

  flush(): void {
    if (this.samples.length === 0) return;
    const samples = this.samples;
    this.samples = [];
    this.send({ type: 'gyro', samples });
  }

  /** Drop anything buffered (connection lost). Timestamps keep increasing, so a new connection is still valid. */
  clear(): void {
    this.samples = [];
  }
}
