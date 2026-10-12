// Camera pointing on/off state for the UI: owns the camera and motion streamers and the PC's feedback.

import type { GyroMessage, TrackingMessage } from '../protocol/types';
import { CameraStreamer, MotionStreamer, requestMotionPermission, type FrameSink } from './camera';

export type CameraPointerState =
  | { status: 'off' }
  | { status: 'starting' }
  | { status: 'on'; motion: boolean; feedback: TrackingMessage | null }
  | { status: 'error'; message: string };

export interface CameraPointerDeps {
  sink: FrameSink;
  sendGyro: (msg: GyroMessage) => boolean;
  /** For tests. */
  createCamera?: (sink: FrameSink) => Pick<CameraStreamer, 'start' | 'stop'>;
  requestMotion?: () => Promise<boolean>;
}

function describeCameraError(e: unknown): string {
  const name = e instanceof DOMException ? e.name : '';
  switch (name) {
    case 'NotAllowedError':
      return 'Camera access was blocked. Allow the camera for this page in the browser settings, then try again.';
    case 'NotFoundError':
    case 'OverconstrainedError':
      return 'No usable camera was found on this phone.';
    case 'NotReadableError':
      return 'The camera is in use by another app.';
    default:
      return 'The camera could not be started.';
  }
}

export class CameraPointer {
  private state: CameraPointerState = { status: 'off' };
  private readonly listeners = new Set<() => void>();
  private readonly camera: Pick<CameraStreamer, 'start' | 'stop'>;
  private readonly motion: MotionStreamer;
  private readonly requestMotion: () => Promise<boolean>;
  /** The user turned it on; survives the page being hidden. */
  private wanted = false;
  private generation = 0;

  constructor(deps: CameraPointerDeps) {
    this.camera = deps.createCamera ? deps.createCamera(deps.sink) : new CameraStreamer(deps.sink);
    this.motion = new MotionStreamer(deps.sendGyro);
    this.requestMotion = deps.requestMotion ?? requestMotionPermission;
  }

  readonly getState = (): CameraPointerState => this.state;

  readonly subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };

  /** Call from a tap: iOS only grants motion access inside a user gesture. */
  async enable(): Promise<void> {
    this.wanted = true;
    await this.startHardware();
  }

  disable(): void {
    this.wanted = false;
    this.stopHardware();
    this.setState({ status: 'off' });
  }

  /** Page hidden: release the camera, but remember to come back. */
  suspend(): void {
    if (this.state.status === 'off' || this.state.status === 'error') return;
    this.stopHardware();
    this.setState({ status: 'off' });
  }

  async resume(): Promise<void> {
    if (this.wanted && this.state.status === 'off') await this.startHardware();
  }

  onTracking(msg: TrackingMessage): void {
    if (this.state.status === 'on') this.setState({ ...this.state, feedback: msg });
  }

  /** Connection lost: buffered gyro is stale and the last feedback no longer applies. */
  onConnectionDown(): void {
    this.motion.clear();
    if (this.state.status === 'on') this.setState({ ...this.state, feedback: null });
  }

  private async startHardware(): Promise<void> {
    const gen = ++this.generation;
    this.setState({ status: 'starting' });
    // Ask for motion first, while still inside the tap.
    const motion = await this.requestMotion();
    if (gen !== this.generation) return;
    try {
      await this.camera.start();
    } catch (e) {
      if (gen !== this.generation) return;
      this.wanted = false;
      this.setState({ status: 'error', message: describeCameraError(e) });
      return;
    }
    if (gen !== this.generation) {
      this.camera.stop();
      return;
    }
    if (motion) this.motion.start();
    this.setState({ status: 'on', motion, feedback: null });
  }

  private stopHardware(): void {
    this.generation += 1;
    this.camera.stop();
    this.motion.stop();
  }

  private setState(next: CameraPointerState): void {
    this.state = next;
    for (const l of this.listeners) l();
  }
}
