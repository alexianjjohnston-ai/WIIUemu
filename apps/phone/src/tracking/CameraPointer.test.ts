import { describe, expect, it, vi } from 'vitest';
import type { AimMessage, CalibrateMessage, TrackingMessage, Vec2 } from '../protocol/types';
import { CameraPointer, type AimStore } from './CameraPointer';

function memoryStore(initial: Vec2 | null = null): AimStore & { point: Vec2 | null } {
  return {
    point: initial,
    load() {
      return this.point;
    },
    save(p) {
      this.point = p;
    },
    clear() {
      this.point = null;
    },
  };
}

function setup(opts: { cameraError?: Error; motion?: boolean; stored?: Vec2; connected?: boolean } = {}) {
  const camera = {
    start: vi.fn(() => (opts.cameraError ? Promise.reject(opts.cameraError) : Promise.resolve())),
    stop: vi.fn(),
  };
  const sent: (CalibrateMessage | AimMessage)[] = [];
  const store = memoryStore(opts.stored ?? null);
  const pointer = new CameraPointer({
    sink: { canSendFrame: () => true, sendFrame: () => true },
    sendGyro: () => true,
    sendControl: (msg) => {
      if (opts.connected === false) return false;
      sent.push(msg);
      return true;
    },
    aimStore: store,
    createCamera: () => camera,
    requestMotion: () => Promise.resolve(opts.motion ?? true),
  });
  return { pointer, camera, sent, store };
}

const FEEDBACK: TrackingMessage = { type: 'tracking', state: 'tracking', pointer: { x: 0.5, y: 0.5 }, tags: 9, fixes_per_s: 29 };

describe('CameraPointer', () => {
  it('turns on, shows PC feedback, and turns off', async () => {
    const { pointer, camera } = setup();
    const p = pointer.enable();
    expect(pointer.getState()).toEqual({ status: 'starting' });
    await p;
    expect(pointer.getState()).toEqual({ status: 'on', motion: true, feedback: null, calibration: { status: 'none' } });
    pointer.onTracking(FEEDBACK);
    expect(pointer.getState()).toMatchObject({ status: 'on', feedback: FEEDBACK });
    pointer.onConnectionDown();
    expect(pointer.getState()).toMatchObject({ status: 'on', feedback: null });
    pointer.disable();
    expect(pointer.getState()).toEqual({ status: 'off' });
    expect(camera.stop).toHaveBeenCalled();
  });

  it('runs camera-only when motion access is denied', async () => {
    const { pointer } = setup({ motion: false });
    await pointer.enable();
    expect(pointer.getState()).toMatchObject({ status: 'on', motion: false });
  });

  it('explains a blocked camera and does not retry on resume', async () => {
    const { pointer, camera } = setup({ cameraError: new DOMException('denied', 'NotAllowedError') });
    await pointer.enable();
    expect(pointer.getState()).toMatchObject({ status: 'error', message: expect.stringContaining('blocked') as string });
    await pointer.resume();
    expect(camera.start).toHaveBeenCalledTimes(1);
  });

  it('releases the camera while the page is hidden and restarts it after', async () => {
    const { pointer, camera } = setup();
    await pointer.enable();
    pointer.suspend();
    expect(pointer.getState()).toEqual({ status: 'off' });
    expect(camera.stop).toHaveBeenCalledTimes(1);
    await pointer.resume();
    expect(pointer.getState()).toMatchObject({ status: 'on' });
    expect(camera.start).toHaveBeenCalledTimes(2);
  });

  it('ignores feedback while off, and a disable during start wins', async () => {
    const { pointer, camera } = setup();
    pointer.onTracking(FEEDBACK);
    expect(pointer.getState()).toEqual({ status: 'off' });
    const p = pointer.enable();
    pointer.disable();
    await p;
    expect(pointer.getState()).toEqual({ status: 'off' });
    expect(camera.start).not.toHaveBeenCalled();
  });

  describe('calibration', () => {
    const OK = { type: 'calibration', ok: true, point: { x: 0.6, y: 0.45 }, reason: null } as const;

    it('counts down, asks the PC to measure, then stores and shows the result', async () => {
      vi.useFakeTimers();
      const { pointer, sent, store } = setup();
      await pointer.enable();
      pointer.calibrate();
      expect(pointer.getState()).toMatchObject({ calibration: { status: 'countdown', seconds: 3 } });
      pointer.calibrate(); // a second tap doesn't restart it
      vi.advanceTimersByTime(1000);
      expect(pointer.getState()).toMatchObject({ calibration: { status: 'countdown', seconds: 2 } });
      expect(sent).toEqual([]);
      vi.advanceTimersByTime(2000);
      expect(sent).toEqual([{ type: 'calibrate' }]);
      expect(pointer.getState()).toMatchObject({ calibration: { status: 'measuring' } });
      pointer.onCalibration(OK);
      expect(store.point).toEqual({ x: 0.6, y: 0.45 });
      expect(pointer.getState()).toMatchObject({ calibration: { status: 'saved', justNow: true } });
      vi.advanceTimersByTime(3000);
      expect(pointer.getState()).toMatchObject({ calibration: { status: 'saved', justNow: false } });
      vi.useRealTimers();
    });

    it("reports the PC's reason and keeps the previous point", async () => {
      vi.useFakeTimers();
      const { pointer, store } = setup({ stored: { x: 0.5, y: 0.4 } });
      await pointer.enable();
      pointer.calibrate();
      vi.advanceTimersByTime(3000);
      pointer.onCalibration({ type: 'calibration', ok: false, point: null, reason: 'off_image' });
      expect(pointer.getState()).toMatchObject({ calibration: { status: 'failed', reason: 'off_image' } });
      expect(store.point).toEqual({ x: 0.5, y: 0.4 });
      vi.useRealTimers();
    });

    it('gives up when the PC does not answer, and ignores a late answer', async () => {
      vi.useFakeTimers();
      const { pointer, store } = setup();
      await pointer.enable();
      pointer.calibrate();
      vi.advanceTimersByTime(3000 + 6000);
      expect(pointer.getState()).toMatchObject({ calibration: { status: 'failed', reason: 'no_answer' } });
      pointer.onCalibration(OK);
      expect(store.point).toBeNull();
      vi.useRealTimers();
    });

    it('fails when not connected, and when the connection drops during the countdown or measuring', async () => {
      vi.useFakeTimers();
      const offline = setup({ connected: false });
      await offline.pointer.enable();
      offline.pointer.calibrate();
      vi.advanceTimersByTime(3000);
      expect(offline.pointer.getState()).toMatchObject({ calibration: { status: 'failed', reason: 'not_connected' } });

      for (const wait of [1000, 3000]) {
        const { pointer, sent } = setup();
        await pointer.enable();
        pointer.calibrate();
        vi.advanceTimersByTime(wait);
        pointer.onConnectionDown();
        expect(pointer.getState()).toMatchObject({ calibration: { status: 'failed', reason: 'not_connected' } });
        vi.advanceTimersByTime(10_000);
        expect(pointer.getState()).toMatchObject({ calibration: { status: 'failed', reason: 'not_connected' } });
        expect(sent.length).toBe(wait === 1000 ? 0 : 1); // a cancelled countdown never measures
      }
      vi.useRealTimers();
    });

    it('sends the stored point when the camera starts and after every reconnect', async () => {
      const { pointer, sent } = setup({ stored: { x: 0.55, y: 0.5 } });
      pointer.onConnectionUp(); // camera off: nothing to send yet
      expect(sent).toEqual([]);
      await pointer.enable();
      expect(pointer.getState()).toMatchObject({ calibration: { status: 'saved', justNow: false } });
      pointer.onConnectionDown();
      pointer.onConnectionUp();
      const aim = { type: 'aim', point: { x: 0.55, y: 0.5 } };
      expect(sent).toEqual([aim, aim]);
    });

    it('reset forgets the point and tells the PC to use the image centre', async () => {
      const { pointer, sent, store } = setup({ stored: { x: 0.55, y: 0.5 } });
      await pointer.enable();
      pointer.resetCalibration();
      expect(store.point).toBeNull();
      expect(sent.at(-1)).toEqual({ type: 'aim', point: null });
      expect(pointer.getState()).toMatchObject({ calibration: { status: 'none' } });
    });
  });
});
