// Protocol 2 binary `camera_frame` encoding and frame-size helpers. Pure functions, unit-tested.

import {
  CAMERA_FRAME_HEADER_BYTES,
  CAMERA_HEIGHT_RANGE,
  CAMERA_WIDTH_RANGE,
  MAX_CAMERA_PAYLOAD_BYTES,
} from '../protocol/types';

export interface FrameHeader {
  width: number;
  height: number;
  seq: number;
  /** Capture time on the phone's monotonic clock (performance.now() × 1000), µs. */
  captureUs: number;
}

/** Header + JPEG payload, little-endian, as specified in docs/controller-protocol.md. */
export function encodeCameraFrame(header: FrameHeader, jpeg: Uint8Array): Uint8Array {
  const out = new Uint8Array(CAMERA_FRAME_HEADER_BYTES + jpeg.length);
  const view = new DataView(out.buffer);
  view.setUint8(0, 1); // kind: camera frame
  view.setUint8(1, 1); // format: JPEG
  view.setUint16(2, header.width, true);
  view.setUint16(4, header.height, true);
  view.setUint16(6, 0, true);
  view.setUint32(8, header.seq >>> 0, true);
  view.setBigUint64(12, BigInt(Math.max(0, Math.floor(header.captureUs))), true);
  out.set(jpeg, CAMERA_FRAME_HEADER_BYTES);
  return out;
}

/**
 * Size to scale a camera image to: the long side at most `maxLongSide`, aspect kept, even dimensions, and inside
 * the protocol's width/height ranges. Returns null when the source can't fit (e.g. an extreme aspect ratio).
 */
export function fitFrameSize(
  srcWidth: number,
  srcHeight: number,
  maxLongSide = 640,
): { width: number; height: number } | null {
  if (!(srcWidth > 0 && srcHeight > 0)) return null;
  const scale = Math.min(1, maxLongSide / Math.max(srcWidth, srcHeight));
  const even = (v: number) => Math.max(2, Math.round((v * scale) / 2) * 2);
  const width = even(srcWidth);
  const height = even(srcHeight);
  const ok =
    width >= CAMERA_WIDTH_RANGE[0] &&
    width <= CAMERA_WIDTH_RANGE[1] &&
    height >= CAMERA_HEIGHT_RANGE[0] &&
    height <= CAMERA_HEIGHT_RANGE[1];
  return ok ? { width, height } : null;
}

export const MIN_JPEG_QUALITY = 0.3;
export const MAX_JPEG_QUALITY = 0.8;

/** Adjusts JPEG quality after a frame of `bytes` so frames stay comfortably under the payload limit. */
export function nextJpegQuality(quality: number, bytes: number): number {
  let q = quality;
  if (bytes > MAX_CAMERA_PAYLOAD_BYTES * 0.85) q -= 0.1;
  else if (bytes < MAX_CAMERA_PAYLOAD_BYTES * 0.4) q += 0.02;
  return Math.round(Math.min(MAX_JPEG_QUALITY, Math.max(MIN_JPEG_QUALITY, q)) * 100) / 100;
}
