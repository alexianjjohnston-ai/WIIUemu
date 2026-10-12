// Input engine: holds the pressed buttons and turns them into protocol `input` snapshots.
// Pure TypeScript (no React, no DOM) so it can be unit-tested with fake timers.
//
// Buttons are held by *sources* (typically one per touch pointer, e.g. "p:3"). A button counts as
// pressed while at least one source holds it, so two fingers on the same button, or a finger sliding
// across the D-pad, never produce duplicates or premature releases.

import type { InputMessage } from '../protocol/types';

export type Sender = (msg: InputMessage) => void;

export interface InputEngineOptions {
  /** Max time between snapshots while attached. The protocol requires <= 100 ms. */
  heartbeatMs?: number;
}

export const DEFAULT_HEARTBEAT_MS = 50;

export class InputEngine<B extends string = string> {
  private readonly order: readonly B[];
  private readonly heartbeatMs: number;
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

  /** Release every button without sending anything extra (a change still triggers a normal send). */
  releaseAll(): boolean {
    if (this.holds.size === 0) return false;
    this.holds.clear();
    return this.recompute();
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
    this.clearTimer();
  }

  /** Build the next snapshot (allocates a new seq). */
  snapshot(): InputMessage {
    this.seq += 1;
    return { type: 'input', seq: this.seq, buttons: [...this.buttons], pointer: null };
  }

  private sendNow(): void {
    const sender = this.sender;
    if (!sender) return;
    sender(this.snapshot());
    this.schedule();
  }

  private schedule(): void {
    this.clearTimer();
    if (!this.sender) return;
    this.timer = setTimeout(() => {
      this.timer = null;
      this.sendNow();
    }, this.heartbeatMs);
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
