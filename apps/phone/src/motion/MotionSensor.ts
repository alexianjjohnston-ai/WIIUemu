// Phone motion sensors → the latest protocol `motion` sample. Owns the devicemotion listener.

import type { Motion } from '../protocol/types';
import { requestMotionPermission } from '../tracking/camera';
import { detectConvention, readingToMotion, type MotionConvention, type MotionGrip } from './convert';

export type MotionStatus = 'off' | 'on' | 'denied' | 'unsupported';

export interface MotionSensorDeps {
  /** Called for every converted sample (e.g. to send a snapshot soon). */
  onSample?: () => void;
  /** For tests. */
  target?: Pick<Window, 'addEventListener' | 'removeEventListener'>;
  requestPermission?: () => Promise<boolean>;
  screenAngle?: () => number;
  convention?: MotionConvention;
  supported?: boolean;
}

/** `screen.orientation.angle`, falling back to the legacy `window.orientation` (older iOS). */
export function currentScreenAngle(): number {
  if (typeof window === 'undefined') return 0;
  const angle =
    typeof screen !== 'undefined' ? (screen.orientation as ScreenOrientation | undefined)?.angle : undefined;
  const legacy = (window as unknown as { orientation?: number }).orientation;
  const raw = typeof angle === 'number' ? angle : typeof legacy === 'number' ? legacy : 0;
  return (((Math.round(raw / 90) * 90) % 360) + 360) % 360;
}

export class MotionSensor {
  private status: MotionStatus = 'off';
  private latest: Motion | null = null;
  private listening = false;
  private readonly listeners = new Set<() => void>();
  private readonly target: MotionSensorDeps['target'];
  private readonly requestPermission: () => Promise<boolean>;
  private readonly screenAngle: () => number;
  private readonly convention: MotionConvention;
  private readonly supported: boolean;
  /** Samples received since the sensor was turned on (diagnostics and tests). */
  samples = 0;

  constructor(
    private grip: MotionGrip,
    private readonly deps: MotionSensorDeps = {},
  ) {
    this.target = deps.target ?? (typeof window === 'undefined' ? undefined : window);
    this.requestPermission = deps.requestPermission ?? requestMotionPermission;
    this.screenAngle = deps.screenAngle ?? currentScreenAngle;
    this.convention = deps.convention ?? detectConvention();
    this.supported = deps.supported ?? (typeof window !== 'undefined' && 'DeviceMotionEvent' in window);
  }

  readonly getStatus = (): MotionStatus => this.status;

  readonly subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };

  /** Latest sample, or null when off or nothing arrived yet. Read by the input engine for every snapshot. */
  readonly getLatest = (): Motion | null => (this.status === 'on' ? this.latest : null);

  setGrip(grip: MotionGrip): void {
    this.grip = grip;
  }

  /** Call from a tap: iOS only grants motion access inside a user gesture. */
  async enable(): Promise<void> {
    if (!this.supported || !this.target) {
      this.setStatus('unsupported');
      return;
    }
    if (!(await this.requestPermission())) {
      this.setStatus('denied');
      return;
    }
    this.latest = null;
    this.samples = 0;
    if (!this.listening) {
      this.target.addEventListener('devicemotion', this.onMotion as EventListener);
      this.listening = true;
    }
    this.setStatus('on');
  }

  disable(): void {
    if (this.listening) this.target?.removeEventListener('devicemotion', this.onMotion as EventListener);
    this.listening = false;
    this.latest = null;
    this.setStatus('off');
  }

  private readonly onMotion = (e: DeviceMotionEvent) => {
    const sample = readingToMotion(e, this.convention, this.screenAngle(), this.grip);
    if (!sample) return;
    this.latest = sample;
    this.samples += 1;
    this.deps.onSample?.();
  };

  private setStatus(status: MotionStatus): void {
    if (this.status === status) return;
    this.status = status;
    for (const l of this.listeners) l();
  }
}
