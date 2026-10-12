import { describe, expect, it } from 'vitest';
import type { GyroMessage } from '../protocol/types';
import { validateCameraFrame, validateGyro } from '../protocol/validate';
import { encodeCameraFrame, fitFrameSize, MAX_JPEG_QUALITY, MIN_JPEG_QUALITY, nextJpegQuality } from './frame';
import { GyroBatcher, rotationRateToRad } from './gyro';

describe('encodeCameraFrame', () => {
  it('writes the little-endian header from the spec', () => {
    const jpeg = new Uint8Array([0xff, 0xd8, 0xff, 0xd9]);
    const bytes = encodeCameraFrame({ width: 640, height: 360, seq: 0x01020304, captureUs: 0x0102030405 }, jpeg);
    expect(Array.from(bytes.subarray(0, 20))).toEqual([
      1, 1, 0x80, 0x02, 0x68, 0x01, 0, 0, 0x04, 0x03, 0x02, 0x01, 0x05, 0x04, 0x03, 0x02, 0x01, 0, 0, 0,
    ]);
    expect(Array.from(bytes.subarray(20))).toEqual(Array.from(jpeg));
    expect(validateCameraFrame(bytes)).toEqual([]);
  });

  it('the validator catches bad headers and non-increasing seq', () => {
    const ok = encodeCameraFrame({ width: 640, height: 360, seq: 5, captureUs: 1 }, new Uint8Array(10));
    expect(validateCameraFrame(ok, 5)).toContain('seq must strictly increase');
    const tiny = encodeCameraFrame({ width: 100, height: 360, seq: 6, captureUs: 1 }, new Uint8Array(10));
    expect(validateCameraFrame(tiny)).toEqual(['width 100 out of range']);
    const empty = encodeCameraFrame({ width: 640, height: 360, seq: 7, captureUs: 1 }, new Uint8Array(0));
    expect(validateCameraFrame(empty)).toEqual(['payload is 0 bytes']);
  });
});

describe('fitFrameSize', () => {
  it('scales the long side to 960 by default and keeps even dimensions', () => {
    expect(fitFrameSize(1920, 1080)).toEqual({ width: 960, height: 540 });
    expect(fitFrameSize(1080, 1920)).toEqual({ width: 540, height: 960 });
    expect(fitFrameSize(640, 480)).toEqual({ width: 640, height: 480 });
    expect(fitFrameSize(1279, 721, 640)).toEqual({ width: 640, height: 360 });
  });

  it('never upscales and rejects sizes the server would reject', () => {
    expect(fitFrameSize(320, 240)).toEqual({ width: 320, height: 240 });
    expect(fitFrameSize(100, 80)).toBeNull();
    expect(fitFrameSize(4000, 200)).toBeNull();
    expect(fitFrameSize(0, 0)).toBeNull();
  });
});

describe('nextJpegQuality', () => {
  it('backs off near the payload limit and creeps up when there is room', () => {
    expect(nextJpegQuality(0.7, 60_000)).toBe(0.6);
    expect(nextJpegQuality(0.7, 10_000)).toBe(0.72);
    expect(nextJpegQuality(0.7, 40_000)).toBe(0.7);
    expect(nextJpegQuality(MIN_JPEG_QUALITY, 70_000)).toBe(MIN_JPEG_QUALITY);
    expect(nextJpegQuality(MAX_JPEG_QUALITY, 1_000)).toBe(MAX_JPEG_QUALITY);
  });
});

describe('rotationRateToRad', () => {
  it('maps beta/gamma/alpha (deg/s) to device x/y/z (rad/s)', () => {
    const r = rotationRateToRad({ alpha: 180, beta: 90, gamma: -45 });
    expect(r?.[0]).toBeCloseTo(Math.PI / 2);
    expect(r?.[1]).toBeCloseTo(-Math.PI / 4);
    expect(r?.[2]).toBeCloseTo(Math.PI);
  });

  it('skips missing or out-of-range values instead of clamping', () => {
    expect(rotationRateToRad(null)).toBeNull();
    expect(rotationRateToRad({ alpha: null, beta: 1, gamma: 1 })).toBeNull();
    expect(rotationRateToRad({ alpha: 3000, beta: 0, gamma: 0 })).toBeNull();
    expect(rotationRateToRad({ alpha: Number.NaN, beta: 0, gamma: 0 })).toBeNull();
  });
});

describe('GyroBatcher', () => {
  function collect() {
    const sent: GyroMessage[] = [];
    return { sent, send: (m: GyroMessage) => (sent.push(m), true) };
  }

  it('flushes once the oldest sample is maxAge old, with valid messages', () => {
    const { sent, send } = collect();
    const b = new GyroBatcher(send, { maxAgeUs: 16_000 });
    b.add(1_000, [0.1, 0.2, 0.3]);
    b.add(9_000, [0.1, 0.2, 0.3]);
    expect(sent).toHaveLength(0);
    b.add(17_000, [0.1, 0.2, 0.3]);
    expect(sent).toHaveLength(1);
    expect(sent[0]?.samples.map((s) => s[0])).toEqual([1_000, 9_000, 17_000]);
    expect(validateGyro(sent[0])).toEqual([]);
  });

  it('drops duplicate and backwards timestamps, keeping later messages valid', () => {
    const { sent, send } = collect();
    const b = new GyroBatcher(send, { maxAgeUs: 0 });
    b.add(100.7, [0, 0, 0]);
    b.add(100.2, [0, 0, 0]);
    b.add(50, [0, 0, 0]);
    b.add(200, [0, 0, 0]);
    expect(sent.map((m) => m.samples.map((s) => s[0]))).toEqual([[100], [200]]);
    let last: number | null = null;
    for (const m of sent) {
      expect(validateGyro(m, last)).toEqual([]);
      last = m.samples.at(-1)?.[0] ?? last;
    }
  });

  it('caps a batch at maxSamples and fits the 4 KiB text frame limit', () => {
    const { sent, send } = collect();
    const b = new GyroBatcher(send, { maxAgeUs: 1e9, maxSamples: 64 });
    for (let i = 1; i <= 64; i++) b.add(1_000_000_000_000 + i, [-34.99999, 34.99999, -12.34567]);
    expect(sent).toHaveLength(1);
    expect(new TextEncoder().encode(JSON.stringify(sent[0])).length).toBeLessThanOrEqual(4096);
  });

  it('clear() discards buffered samples', () => {
    const { sent, send } = collect();
    const b = new GyroBatcher(send, { maxAgeUs: 1e9 });
    b.add(1, [0, 0, 0]);
    b.clear();
    b.flush();
    expect(sent).toHaveLength(0);
  });
});
