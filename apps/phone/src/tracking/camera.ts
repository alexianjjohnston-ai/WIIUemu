// Browser glue for camera pointing (protocol 2): rear camera → small JPEG frames, DeviceMotion → gyro batches.
// The encoding rules live in frame.ts / gyro.ts (unit-tested). This file only talks to browser APIs and is
// exercised by the Playwright tests with Chromium's fake camera.

import { MAX_CAMERA_PAYLOAD_BYTES, type GyroMessage } from '../protocol/types';
import { DEFAULT_MAX_LONG_SIDE, encodeCameraFrame, fitFrameSize, nextJpegQuality } from './frame';
import { GyroBatcher, rotationRateToRad } from './gyro';

export interface FrameSink {
  canSendFrame(): boolean;
  sendFrame(frame: Uint8Array): boolean;
}

/** Camera pointing needs a secure page (https or localhost) and the camera API. */
export function cameraPointingSupported(): boolean {
  // `mediaDevices` is missing entirely on insecure pages, whatever lib.dom says.
  const media = navigator.mediaDevices as MediaDevices | undefined;
  return typeof window !== 'undefined' && window.isSecureContext && typeof media?.getUserMedia === 'function';
}

/** Older Safari lacks requestVideoFrameCallback even though lib.dom declares it. */
function hasFrameCallback(video: HTMLVideoElement): boolean {
  return typeof (video as Partial<HTMLVideoElement>).requestVideoFrameCallback === 'function';
}

export interface CameraStreamerOptions {
  targetFps?: number;
  maxLongSide?: number;
}

/** Streams the rear camera as protocol-2 `camera_frame`s, never more than one frame in flight. */
export class CameraStreamer {
  private stream: MediaStream | null = null;
  private video: HTMLVideoElement | null = null;
  private timer: ReturnType<typeof setTimeout> | null = null;
  private frameHandle: number | null = null;
  private running = false;
  private busy = false;
  private seq = 0;
  private quality = 0.7;
  private lastSentMs = Number.NEGATIVE_INFINITY;
  private readonly frameIntervalMs: number;
  private readonly maxLongSide: number;
  framesSent = 0;

  constructor(
    private readonly sink: FrameSink,
    opts: CameraStreamerOptions = {},
  ) {
    this.frameIntervalMs = 1000 / (opts.targetFps ?? 30);
    this.maxLongSide = opts.maxLongSide ?? DEFAULT_MAX_LONG_SIDE;
  }

  /** Opens the rear camera. Rejects with the getUserMedia error (e.g. NotAllowedError) on failure. */
  async start(): Promise<void> {
    if (this.running) return;
    this.running = true;
    let stream: MediaStream;
    try {
      stream = await navigator.mediaDevices.getUserMedia({
        audio: false,
        video: {
          facingMode: { ideal: 'environment' },
          // Ask for a wide mode (cropping to 4:3 would cut tags off the sides); frames are downscaled before sending.
          width: { ideal: 1280 },
          height: { ideal: 720 },
          frameRate: { ideal: 30, max: 30 },
        },
      });
    } catch (e) {
      this.running = false;
      throw e;
    }
    if (this.stopped()) {
      for (const t of stream.getTracks()) t.stop();
      return;
    }
    this.stream = stream;
    const video = document.createElement('video');
    video.muted = true;
    video.playsInline = true;
    video.srcObject = stream;
    this.video = video;
    await video.play();
    this.schedule();
  }

  stop(): void {
    this.running = false;
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = null;
    if (this.frameHandle !== null) this.video?.cancelVideoFrameCallback(this.frameHandle);
    this.frameHandle = null;
    for (const t of this.stream?.getTracks() ?? []) t.stop();
    this.stream = null;
    if (this.video) this.video.srcObject = null;
    this.video = null;
  }

  /** Re-reads `running` after an await (stop() may have run meanwhile). */
  private stopped(): boolean {
    return !this.running;
  }

  private schedule(): void {
    const video = this.video;
    if (!this.running || !video) return;
    if (hasFrameCallback(video)) {
      this.frameHandle = video.requestVideoFrameCallback((now, meta) => {
        this.frameHandle = null;
        this.schedule();
        void this.onFrame(meta.captureTime ?? now);
      });
    } else {
      this.timer = setTimeout(() => {
        this.timer = null;
        this.schedule();
        void this.onFrame(performance.now());
      }, this.frameIntervalMs / 2);
    }
  }

  private async onFrame(captureMs: number): Promise<void> {
    const video = this.video;
    if (!video || this.busy || captureMs - this.lastSentMs < this.frameIntervalMs * 0.9) return;
    if (!this.sink.canSendFrame()) return;
    const size = fitFrameSize(video.videoWidth, video.videoHeight, this.maxLongSide);
    if (!size) return;
    this.busy = true;
    try {
      const jpeg = await encodeJpeg(video, size.width, size.height, this.quality);
      if (this.stopped() || !jpeg) return;
      this.quality = nextJpegQuality(this.quality, jpeg.length);
      if (jpeg.length === 0 || jpeg.length > MAX_CAMERA_PAYLOAD_BYTES) return;
      this.seq += 1;
      const frame = encodeCameraFrame({ ...size, seq: this.seq, captureUs: captureMs * 1000 }, jpeg);
      if (this.sink.sendFrame(frame)) {
        this.lastSentMs = captureMs;
        this.framesSent += 1;
      }
    } finally {
      this.busy = false;
    }
  }
}

let offscreen: OffscreenCanvas | null = null;
let fallbackCanvas: HTMLCanvasElement | null = null;

async function encodeJpeg(source: CanvasImageSource, width: number, height: number, quality: number) {
  let blob: Blob | null;
  if (typeof OffscreenCanvas !== 'undefined') {
    offscreen ??= new OffscreenCanvas(width, height);
    if (offscreen.width !== width || offscreen.height !== height) {
      offscreen.width = width;
      offscreen.height = height;
    }
    const ctx = offscreen.getContext('2d');
    if (!ctx) return null;
    ctx.drawImage(source, 0, 0, width, height);
    blob = await offscreen.convertToBlob({ type: 'image/jpeg', quality });
  } else {
    fallbackCanvas ??= document.createElement('canvas');
    const canvas = fallbackCanvas;
    canvas.width = width;
    canvas.height = height;
    const ctx = canvas.getContext('2d');
    if (!ctx) return null;
    ctx.drawImage(source, 0, 0, width, height);
    blob = await new Promise<Blob | null>((resolve) => {
      canvas.toBlob(resolve, 'image/jpeg', quality);
    });
  }
  return blob ? new Uint8Array(await blob.arrayBuffer()) : null;
}

type MotionPermissionApi = { requestPermission?: () => Promise<'granted' | 'denied'> };

/**
 * iOS asks for motion access; it must be requested from a tap. Resolves true when motion events may arrive
 * (Android and desktop browsers don't ask).
 */
export async function requestMotionPermission(): Promise<boolean> {
  if (typeof DeviceMotionEvent === 'undefined') return false;
  const api = DeviceMotionEvent as unknown as MotionPermissionApi;
  if (typeof api.requestPermission !== 'function') return true;
  try {
    return (await api.requestPermission()) === 'granted';
  } catch {
    return false;
  }
}

/** Streams DeviceMotionEvent.rotationRate as protocol-2 `gyro` batches. */
export class MotionStreamer {
  private readonly batcher: GyroBatcher;
  private listening = false;
  samples = 0;

  constructor(send: (msg: GyroMessage) => boolean) {
    this.batcher = new GyroBatcher(send);
  }

  private readonly onMotion = (e: DeviceMotionEvent) => {
    const rate = rotationRateToRad(e.rotationRate);
    if (!rate) return;
    this.samples += 1;
    // Event.timeStamp shares performance.now()'s clock with the camera capture time.
    this.batcher.add(e.timeStamp * 1000, rate);
  };

  start(): void {
    if (this.listening || typeof window === 'undefined') return;
    window.addEventListener('devicemotion', this.onMotion);
    this.listening = true;
  }

  stop(): void {
    if (this.listening) window.removeEventListener('devicemotion', this.onMotion);
    this.listening = false;
    this.batcher.clear();
  }

  /** The connection dropped: buffered samples are stale. */
  clear(): void {
    this.batcher.clear();
  }
}
