// Input engine: holds the pressed buttons and turns them into protocol `input` snapshots.
// Pure TypeScript (no React, no DOM) so it can be unit-tested with fake timers.
//
// Buttons are held by *sources* (typically one per touch pointer, e.g. "p:3"). A button counts as
// pressed while at least one source holds it, so two fingers on the same button, or a finger sliding
// across the D-pad, never produce duplicates or premature releases.

import type { InputMessage, Motion, Vec2 } from '../protocol/types';

export type Sender = (msg: InputMessage) => void;

export interface InputEngineOptions {
  /** Max time between snapshots while attached. The protocol requires <= 100 ms. */
  heartbeatMs?: number;
  /** Include `stick` and `rstick` in every snapshot (GamePad). */
  sticks?: boolean;
  /** Analog changes (sticks, touch, motion) are coalesced to at most one snapshot per this many ms. */
  analogIntervalMs?: number;
  /** For tests. */
  now?: () => number;
}

export const DEFAULT_HEARTBEAT_MS = 50;
export const DEFAULT_ANALOG_INTERVAL_MS = 16;

export type StickName = 'stick' | 'rstick';

const CENTRE: Vec2 = { x: 0, y: 0 };

export class InputEngine<B extends string = string> {
  private readonly order: readonly B[];
  private readonly heartbeatMs: number;
  private readonly analogIntervalMs: number;
  private readonly sticks: boolean;
  private readonly now: () => number;
  private stick: Vec2 = CENTRE;
  private rstick: Vec2 = CENTRE;
  private pointer: Vec2 | null = null;
  private motionSource: (() => Motion | null) | null = null;
  private lastSentAt = -Infinity;
  /** An analog change is waiting for the coalescing interval to pass. */
  private analogPending = false;
  private readonly holds = new Map<string, ReadonlySet<B>>();
  private readonly listeners = new Set<() => void>();
  private buttons: readonly B[] = [];
  private seq = 0;
  private sender: Sender | null = null;
  private timer: ReturnType<typeof setTimeout> | null = null;

  /** @param order every valid button, in the order snapshots list them */
  constructor(order: readonly B[], options: InputEngineOptions = {}) {
    this.order = order;
    this.heartbeatMs = options.heartbeatMs ?? DEFAULT_HEARTBEAT_MS;
    this.analogIntervalMs = options.analogIntervalMs ?? DEFAULT_ANALOG_INTERVAL_MS;
    this.sticks = options.sticks ?? false;
    this.now = options.now ?? (() => Date.now());
  }

  // ---- state -------------------------------------------------------------------------------

  /** Currently pressed buttons in canonical order. Same array instance until the set changes. */
  readonly getButtons = (): readonly B[] => this.buttons;

  isPressed(button: B): boolean {
    return this.buttons.includes(button);
  }

  /** Last seq that was sent (0 if nothing was sent yet). */
  get lastSeq(): number {
    return this.seq;
  }

  get attached(): boolean {
    return this.sender !== null;
  }

  /** Subscribe to changes of the pressed set. Returns an unsubscribe function. */
  readonly subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };

  // ---- mutations ---------------------------------------------------------------------------

  /** Replace everything `source` holds. Returns true if the overall pressed set changed. */
  setSource(source: string, buttons: Iterable<B>): boolean {
    const set = new Set(buttons);
    if (set.size === 0) this.holds.delete(source);
    else this.holds.set(source, set);
    return this.recompute();
  }

  press(source: string, button: B): boolean {
    const current = this.holds.get(source);
    if (current?.has(button)) return false;
    return this.setSource(source, [...(current ?? []), button]);
  }

  release(source: string, button: B): boolean {
    const current = this.holds.get(source);
    if (!current?.has(button)) return false;
    return this.setSource(
      source,
      [...current].filter((b) => b !== button),
    );
  }

  releaseSource(source: string): boolean {
    if (!this.holds.has(source)) return false;
    this.holds.delete(source);
    return this.recompute();
  }

  /**
   * Release every button, centre both sticks and lift the touch, without sending anything extra (a change still
   * triggers a normal send).
   */
  releaseAll(): boolean {
    const analogHeld = this.pointer !== null || !isCentre(this.stick) || !isCentre(this.rstick);
    this.stick = CENTRE;
    this.rstick = CENTRE;
    this.pointer = null;
    this.holds.clear();
    if (this.recompute()) return true;
    if (analogHeld) this.sendNow();
    return analogHeld;
  }

  // ---- analog ------------------------------------------------------------------------------

  getStick(which: StickName): Vec2 {
    return which === 'stick' ? this.stick : this.rstick;
  }

  getPointer(): Vec2 | null {
    return this.pointer;
  }

  /** Set a stick (`x`, `y` in [-1, 1], +y up). The caller keeps values in range. */
  setStick(which: StickName, value: Vec2): void {
    const prev = this.getStick(which);
    if (prev.x === value.x && prev.y === value.y) return;
    if (which === 'stick') this.stick = value;
    else this.rstick = value;
    this.analogChanged();
  }

  /** Touch position (`x`, `y` in [0, 1] of the touch area) or null when lifted. */
  setPointer(value: Vec2 | null): void {
    const prev = this.pointer;
    if (prev === value || (prev && value && prev.x === value.x && prev.y === value.y)) return;
    const lifted = value === null || prev === null;
    this.pointer = value;
    // Touching down or lifting is like a button edge: send at once. Moves are coalesced.
    if (lifted) this.sendNow();
    else this.analogChanged();
  }

  /** Motion is read from `source` for every snapshot. `null` stops sending motion. */
  setMotionSource(source: (() => Motion | null) | null): void {
    this.motionSource = source;
  }

  /** A new motion sample arrived: send it soon (coalesced like other analog input). */
  motionChanged(): void {
    if (this.motionSource) this.analogChanged();
  }

  /**
   * Release everything and make sure the server sees exactly one final empty snapshot, whether or
   * not anything was pressed. Used on page hide / blur. No-op on the wire when detached.
   */
  releaseAllAndFlush(): void {
    if (!this.releaseAll()) this.sendNow();
  }

  // ---- transport ---------------------------------------------------------------------------

  /** Start sending: one snapshot now, then on every change and at least every heartbeatMs. */
  attach(sender: Sender): void {
    this.sender = sender;
    this.sendNow();
  }

  /** Stop sending (socket closed). Does not change the pressed set. */
  detach(): void {
    this.sender = null;
    this.analogPending = false;
    this.clearTimer();
  }

  /** Build the next snapshot (allocates a new seq). */
  snapshot(): InputMessage {
    this.seq += 1;
    const msg: InputMessage = {
      type: 'input',
      seq: this.seq,
      buttons: [...this.buttons],
      pointer: this.pointer,
    };
    if (this.sticks) {
      msg.stick = this.stick;
      msg.rstick = this.rstick;
    }
    const motion = this.motionSource?.();
    if (motion) msg.motion = motion;
    return msg;
  }

  private sendNow(): void {
    const sender = this.sender;
    this.analogPending = false;
    if (!sender) return;
    sender(this.snapshot());
    this.lastSentAt = this.now();
    this.schedule(this.heartbeatMs);
  }

  private analogChanged(): void {
    if (!this.sender || this.analogPending) return;
    const wait = this.lastSentAt + this.analogIntervalMs - this.now();
    if (wait <= 0) {
      this.sendNow();
      return;
    }
    this.analogPending = true;
    this.schedule(wait);
  }

  private schedule(ms: number): void {
    this.clearTimer();
    if (!this.sender) return;
    this.timer = setTimeout(() => {
      this.timer = null;
      this.sendNow();
    }, ms);
  }

  private clearTimer(): void {
    if (this.timer !== null) {
      clearTimeout(this.timer);
      this.timer = null;
    }
  }

  private recompute(): boolean {
    const held = new Set<B>();
    for (const set of this.holds.values()) for (const b of set) held.add(b);
    const next = this.order.filter((b) => held.has(b));
    const prev = this.buttons;
    if (next.length === prev.length && next.every((b, i) => b === prev[i])) return false;
    this.buttons = next;
    for (const listener of this.listeners) listener();
    this.sendNow();
    return true;
  }
}

function isCentre(v: Vec2): boolean {
  return v.x === 0 && v.y === 0;
}
