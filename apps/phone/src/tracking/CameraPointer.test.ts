import { describe, expect, it, vi } from 'vitest';
import type { TrackingMessage } from '../protocol/types';
import { CameraPointer } from './CameraPointer';

function setup(opts: { cameraError?: Error; motion?: boolean } = {}) {
  const camera = {
    start: vi.fn(() => (opts.cameraError ? Promise.reject(opts.cameraError) : Promise.resolve())),
    stop: vi.fn(),
  };
  const pointer = new CameraPointer({
    sink: { canSendFrame: () => true, sendFrame: () => true },
    sendGyro: () => true,
    createCamera: () => camera,
    requestMotion: () => Promise.resolve(opts.motion ?? true),
  });
  return { pointer, camera };
}

const FEEDBACK: TrackingMessage = { type: 'tracking', state: 'tracking', pointer: { x: 0.5, y: 0.5 }, tags: 9, fixes_per_s: 29 };

describe('CameraPointer', () => {
  it('turns on, shows PC feedback, and turns off', async () => {
    const { pointer, camera } = setup();
    const p = pointer.enable();
    expect(pointer.getState()).toEqual({ status: 'starting' });
    await p;
    expect(pointer.getState()).toEqual({ status: 'on', motion: true, feedback: null });
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
});
