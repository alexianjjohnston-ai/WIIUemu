// Camera pointing on/off state for the UI: owns the camera and motion streamers and the PC's feedback.

import type {
  AimMessage,
  CalibrateMessage,
  CalibrationMessage,
  GyroMessage,
  TrackingMessage,
  Vec2,
} from '../protocol/types';
import { isAimPoint } from '../protocol/validate';
import { CameraStreamer, MotionStreamer, requestMotionPermission, type FrameSink } from './camera';

/** Pointer calibration (docs/controller-protocol.md, "Pointer calibration"). */
export type CalibrationStatus =
  /** Not calibrated: the PC uses the camera image centre. */
  | { status: 'none' }
  /** Time to aim after tapping Calibrate; the PC measures when it reaches 0. */
  | { status: 'countdown'; seconds: number }
  | { status: 'measuring' }
  /** `justNow`: the success message is still showing. */
  | { status: 'saved'; justNow: boolean }
  /** `reason`: the PC's `no_markers` / `off_image`, or `no_answer` / `not_connected` from the phone. */
  | { status: 'failed'; reason: string };

export type CameraPointerState =
  | { status: 'off' }
  | { status: 'starting' }
  | { status: 'on'; motion: boolean; feedback: TrackingMessage | null; calibration: CalibrationStatus }
  | { status: 'error'; message: string };

/** Where this phone's aim point is kept (it belongs to this phone's camera, so it stays on the phone). */
export interface AimStore {
  load(): Vec2 | null;
  save(point: Vec2): void;
  clear(): void;
}

const AIM_KEY = 'wiiu-connect:aim-point';
/** The PC answers within its 3 s fix timeout; allow for the network. */
const CALIBRATION_ANSWER_MS = 6000;
const SUCCESS_SHOWN_MS = 3000;
export const CALIBRATION_COUNTDOWN_S = 3;

/** localStorage-backed store; storage can be unavailable (private mode), which just means no calibration. */
export const localAimStore: AimStore = {
  load() {
    try {
      const raw = localStorage.getItem(AIM_KEY);
      const p: unknown = raw ? JSON.parse(raw) : null;
      return isAimPoint(p) ? { x: p.x, y: p.y } : null;
    } catch {
      return null;
    }
  },
  save(point) {
    try {
      localStorage.setItem(AIM_KEY, JSON.stringify(point));
    } catch {
      /* not stored: calibration lasts until the page closes */
    }
  },
  clear() {
    try {
      localStorage.removeItem(AIM_KEY);
    } catch {
      /* nothing stored */
    }
  },
};

export interface CameraPointerDeps {
  sink: FrameSink;
  sendGyro: (msg: GyroMessage) => boolean;
  /** Sends `calibrate` / `aim`; false when not connected. */
  sendControl: (msg: CalibrateMessage | AimMessage) => boolean;
  aimStore?: AimStore;
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
  private readonly sendControl: CameraPointerDeps['sendControl'];
  private readonly aimStore: AimStore;
  private calibrationTimer: ReturnType<typeof setTimeout> | null = null;

  constructor(deps: CameraPointerDeps) {
    this.camera = deps.createCamera ? deps.createCamera(deps.sink) : new CameraStreamer(deps.sink);
    this.motion = new MotionStreamer(deps.sendGyro);
    this.requestMotion = deps.requestMotion ?? requestMotionPermission;
    this.sendControl = deps.sendControl;
    this.aimStore = deps.aimStore ?? localAimStore;
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

  /** Start calibrating: a short countdown to aim at the cross in the middle of the TV, then the PC measures. */
  calibrate(): void {
    const c = this.calibration();
    if (!c || c.status === 'countdown' || c.status === 'measuring') return;
    this.countdown(CALIBRATION_COUNTDOWN_S);
  }

  /** Forget this phone's calibration and go back to the camera image centre. */
  resetCalibration(): void {
    this.aimStore.clear();
    this.sendControl({ type: 'aim', point: null });
    if (this.state.status === 'on') this.setCalibration({ status: 'none' });
  }

  onCalibration(msg: CalibrationMessage): void {
    if (this.calibration()?.status !== 'measuring') return; // late answer to a request we gave up on
    if (msg.ok) {
      this.aimStore.save(msg.point);
      this.setCalibration({ status: 'saved', justNow: true });
      this.startCalibrationTimer(SUCCESS_SHOWN_MS, () => {
        if (this.calibration()?.status === 'saved') this.setCalibration({ status: 'saved', justNow: false });
      });
    } else {
      this.setCalibration({ status: 'failed', reason: msg.reason });
    }
  }

  /** A new connection starts uncalibrated on the PC: send this phone's aim point again. */
  onConnectionUp(): void {
    if (this.state.status === 'on') this.sendStoredAim();
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

  /** Connection lost: buffered gyro is stale, the last feedback no longer applies, and a calibration stops. */
  onConnectionDown(): void {
    this.motion.clear();
    if (this.state.status !== 'on') return;
    const pending = this.state.calibration.status === 'measuring' || this.state.calibration.status === 'countdown';
    if (pending) this.clearCalibrationTimer();
    const calibration: CalibrationStatus = pending ? { status: 'failed', reason: 'not_connected' } : this.state.calibration;
    this.setState({ ...this.state, feedback: null, calibration });
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
    const stored = this.sendStoredAim();
    this.setState({
      status: 'on',
      motion,
      feedback: null,
      calibration: stored ? { status: 'saved', justNow: false } : { status: 'none' },
    });
  }

  private countdown(seconds: number): void {
    if (seconds <= 0) {
      this.measure();
      return;
    }
    this.setCalibration({ status: 'countdown', seconds });
    this.startCalibrationTimer(1000, () => {
      if (this.calibration()?.status === 'countdown') this.countdown(seconds - 1);
    });
  }

  /** The player is aiming at the cross now: ask the PC to measure the aim point. */
  private measure(): void {
    if (!this.sendControl({ type: 'calibrate' })) {
      this.setCalibration({ status: 'failed', reason: 'not_connected' });
      return;
    }
    this.setCalibration({ status: 'measuring' });
    this.startCalibrationTimer(CALIBRATION_ANSWER_MS, () => {
      if (this.calibration()?.status === 'measuring') this.setCalibration({ status: 'failed', reason: 'no_answer' });
    });
  }

  /** Sends the stored aim point, if any; returns whether one is stored. */
  private sendStoredAim(): boolean {
    const point = this.aimStore.load();
    if (point) this.sendControl({ type: 'aim', point });
    return point !== null;
  }

  private calibration(): CalibrationStatus | null {
    return this.state.status === 'on' ? this.state.calibration : null;
  }

  private setCalibration(calibration: CalibrationStatus): void {
    if (this.state.status === 'on') this.setState({ ...this.state, calibration });
  }

  private startCalibrationTimer(ms: number, fn: () => void): void {
    this.clearCalibrationTimer();
    this.calibrationTimer = setTimeout(() => {
      this.calibrationTimer = null;
      fn();
    }, ms);
  }

  private clearCalibrationTimer(): void {
    if (this.calibrationTimer !== null) clearTimeout(this.calibrationTimer);
    this.calibrationTimer = null;
  }

  private stopHardware(): void {
    this.generation += 1;
    this.clearCalibrationTimer();
    this.camera.stop();
    this.motion.stop();
  }

  private setState(next: CameraPointerState): void {
    this.state = next;
    for (const l of this.listeners) l();
  }
}
