// WebSocket connection to the PC: handshake, keep-alive pings, automatic reconnect with backoff.
// Pure TypeScript; the WebSocket constructor is injectable so the state machine can be unit-tested.

import {
  MIN_PROTOCOL_VERSION,
  PROTOCOL_VERSION,
  type CalibrationMessage,
  type ClientMessage,
  type HelloMessage,
  type TrackingMessage,
  type WelcomeMessage,
} from '../protocol/types';
import { parseServerMessage } from '../protocol/validate';
import { backoffDelay, DEFAULT_BACKOFF, type BackoffOptions } from './backoff';
import { isTerminal, type ConnError } from './errors';

export type ConnectionState =
  | { status: 'idle' }
  | { status: 'connecting'; attempt: number }
  | { status: 'connected'; welcome: WelcomeMessage; rttMs: number | null }
  | { status: 'reconnecting'; attempt: number; retryInMs: number; reason: ConnError }
  | { status: 'failed'; reason: ConnError };

/** The subset of the WebSocket API this module uses. */
export interface SocketLike {
  readonly readyState: number;
  readonly bufferedAmount: number;
  onopen: ((ev: Event) => void) | null;
  onmessage: ((ev: MessageEvent) => void) | null;
  onclose: ((ev: CloseEvent) => void) | null;
  onerror: ((ev: Event) => void) | null;
  binaryType?: BinaryType;
  send(data: string | Uint8Array): void;
  close(code?: number, reason?: string): void;
}

export type SocketFactory = (url: string) => SocketLike;

const OPEN = 1;
/**
 * Skip input snapshots only while this much data is still queued (a congested link): snapshots are idempotent, so
 * newer ones win. Leaves room for one in-flight camera frame so frames never delay button input.
 */
const MAX_BUFFERED_BYTES = 128 * 1024;
/** Send a camera frame only once the previous one has mostly left the phone, so frames never queue up. */
const MAX_BUFFERED_BEFORE_FRAME = 16 * 1024;

export interface ConnectionOptions {
  url: string;
  hello: Omit<HelloMessage, 'type' | 'protocol'>;
  createSocket?: SocketFactory;
  backoff?: BackoffOptions;
  random?: () => number;
  /** Close and retry if no `welcome` arrives this long after starting an attempt. */
  handshakeTimeoutMs?: number;
  pingIntervalMs?: number;
  /** Treat the link as dead if nothing at all arrives from the server for this long. */
  staleAfterMs?: number;
  now?: () => number;
}

export interface ConnectionEvents {
  /** Handshake finished; input may be sent now. */
  onReady?: () => void;
  /** The socket that was ready went away (always paired with a previous onReady). */
  onDown?: () => void;
  /** Protocol 2 camera-tracking feedback from the PC. */
  onTracking?: (msg: TrackingMessage) => void;
  /** Protocol 2: result of a `calibrate` request. */
  onCalibration?: (msg: CalibrationMessage) => void;
}

export class Connection {
  private readonly opts: Required<Omit<ConnectionOptions, 'hello' | 'url'>> &
    Pick<ConnectionOptions, 'hello' | 'url'>;
  private readonly events: ConnectionEvents;
  private readonly listeners = new Set<() => void>();
  private state: ConnectionState = { status: 'idle' };
  private socket: SocketLike | null = null;
  private ready = false;
  private failures = 0;
  private lastError: ConnError | null = null;
  /** Version sent in `hello`; drops to MIN_PROTOCOL_VERSION once an older PC rejects the newer one. */
  private helloProtocol: number = PROTOCOL_VERSION;
  private retryOlderProtocol = false;
  private lastHeardAt = 0;
  private pingId = 0;
  private pingSentAt = new Map<number, number>();
  private handshakeTimer: ReturnType<typeof setTimeout> | null = null;
  private retryTimer: ReturnType<typeof setTimeout> | null = null;
  private pingTimer: ReturnType<typeof setInterval> | null = null;

  constructor(options: ConnectionOptions, events: ConnectionEvents = {}) {
    this.opts = {
      createSocket: (url) => new WebSocket(url),
      backoff: DEFAULT_BACKOFF,
      random: Math.random,
      handshakeTimeoutMs: 8000,
      pingIntervalMs: 2000,
      staleAfterMs: 6000,
      now: () => Date.now(),
      ...options,
    };
    this.events = events;
  }

  readonly getState = (): ConnectionState => this.state;

  readonly subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };

  /** Begin connecting (no-op if already active). */
  start(): void {
    if (this.state.status === 'idle' || this.state.status === 'failed') {
      this.failures = 0;
      this.helloProtocol = PROTOCOL_VERSION;
      this.open();
    }
  }

  /** Skip the remaining backoff delay, e.g. when the page becomes visible or the network returns. */
  retryNow(): void {
    if (this.state.status === 'reconnecting') {
      this.clearRetry();
      this.open();
    }
  }

  /** Close for good (user left the controller screen). */
  stop(): void {
    this.clearRetry();
    this.teardownSocket(1000, 'client leaving');
    this.setState({ status: 'idle' });
  }

  /** Send a message if the socket is open. Returns true if it was handed to the socket. */
  send(msg: ClientMessage): boolean {
    const ws = this.socket;
    if (!ws || ws.readyState !== OPEN) return false;
    if (msg.type === 'input' && ws.bufferedAmount > MAX_BUFFERED_BYTES) return false;
    if (msg.type === 'gyro' && !this.supportsTracking) return false;
    try {
      ws.send(JSON.stringify(msg));
      return true;
    } catch {
      return false;
    }
  }

  get isReady(): boolean {
    return this.ready;
  }

  /** The PC accepted protocol 2 on this connection, so it takes camera frames and gyro. */
  get supportsTracking(): boolean {
    return this.state.status === 'connected' && this.state.welcome.protocol >= 2;
  }

  /** True when a camera frame sent now would not queue behind an earlier one. */
  canSendFrame(): boolean {
    const ws = this.socket;
    return this.supportsTracking && ws !== null && ws.readyState === OPEN && ws.bufferedAmount <= MAX_BUFFERED_BEFORE_FRAME;
  }

  /** Send an encoded binary `camera_frame`. Returns false (frame dropped) if it can't go out right now. */
  sendFrame(frame: Uint8Array): boolean {
    if (!this.canSendFrame()) return false;
    try {
      this.socket?.send(frame);
      return true;
    } catch {
      return false;
    }
  }

  // ---- internals -----------------------------------------------------------------------------

  private setState(next: ConnectionState): void {
    this.state = next;
    for (const l of this.listeners) l();
  }

  private open(): void {
    this.lastError = null;
    this.setState({ status: 'connecting', attempt: this.failures + 1 });
    let ws: SocketLike;
    try {
      ws = this.opts.createSocket(this.opts.url);
    } catch {
      // Invalid URL etc. Treat like a network failure so the user sees a readable state.
      this.lastError = { code: 'network', detail: 'could not open WebSocket' };
      this.handleClosed();
      return;
    }
    this.socket = ws;
    ws.binaryType = 'arraybuffer';

    ws.onopen = () => {
      const hello: HelloMessage = { type: 'hello', protocol: this.helloProtocol, ...this.opts.hello };
      this.send(hello);
    };
    ws.onmessage = (ev) => {
      this.onMessage(ev.data);
    };
    ws.onerror = () => {
      // A close event always follows; nothing to do here.
    };
    ws.onclose = () => {
      if (this.socket !== ws) return;
      this.detachSocket();
      this.handleClosed();
    };

    this.handshakeTimer = setTimeout(() => {
      this.handshakeTimer = null;
      this.lastError ??= { code: 'handshake_timeout' };
      this.abandonSocket();
    }, this.opts.handshakeTimeoutMs);
  }

  private onMessage(data: unknown): void {
    if (typeof data !== 'string') return;
    this.lastHeardAt = this.opts.now();
    const msg = parseServerMessage(data);
    if (!msg) return;
    switch (msg.type) {
      case 'welcome':
        if (!Number.isInteger(msg.protocol) || msg.protocol < MIN_PROTOCOL_VERSION || msg.protocol > PROTOCOL_VERSION) {
          this.lastError = { code: 'unsupported_protocol', detail: `server protocol ${msg.protocol}` };
          this.abandonSocket();
          return;
        }
        this.clearHandshake();
        this.failures = 0;
        this.ready = true;
        this.setState({ status: 'connected', welcome: msg, rttMs: null });
        this.startPings();
        this.events.onReady?.();
        return;
      case 'pong': {
        const sentAt = this.pingSentAt.get(msg.id);
        this.pingSentAt.delete(msg.id);
        if (sentAt !== undefined && this.state.status === 'connected') {
          this.setState({ ...this.state, rttMs: Math.max(0, this.opts.now() - sentAt) });
        }
        return;
      }
      case 'error':
        if (msg.code === 'unsupported_protocol' && !this.ready && this.helloProtocol > MIN_PROTOCOL_VERSION) {
          // An older PC (protocol 1, no camera pointing): say hello again with the oldest version we speak.
          this.helloProtocol = MIN_PROTOCOL_VERSION;
          this.retryOlderProtocol = true;
          return;
        }
        // The server closes right after sending an error; remember why.
        this.lastError = { code: msg.code, detail: msg.message };
        return;
      case 'tracking':
        if (this.ready) this.events.onTracking?.(msg);
        return;
      case 'calibration':
        if (this.ready) this.events.onCalibration?.(msg);
        return;
    }
  }

  private startPings(): void {
    this.lastHeardAt = this.opts.now();
    this.pingTimer = setInterval(() => {
      const now = this.opts.now();
      if (now - this.lastHeardAt > this.opts.staleAfterMs) {
        this.lastError ??= { code: 'stale' };
        this.abandonSocket();
        return;
      }
      this.pingId += 1;
      this.pingSentAt.set(this.pingId, now);
      if (this.pingSentAt.size > 8) {
        const oldest = this.pingSentAt.keys().next().value;
        if (oldest !== undefined) this.pingSentAt.delete(oldest);
      }
      this.send({ type: 'ping', id: this.pingId });
    }, this.opts.pingIntervalMs);
  }

  /** Give up on the current socket immediately, without waiting for a close handshake. */
  private abandonSocket(): void {
    this.teardownSocket(4000, 'client timeout');
    this.handleClosed();
  }

  private teardownSocket(code: number, reason: string): void {
    const ws = this.socket;
    this.detachSocket();
    if (ws) {
      ws.onopen = ws.onmessage = ws.onclose = ws.onerror = null;
      try {
        ws.close(code, reason);
      } catch {
        // already closed
      }
    }
  }

  /** Forget the socket and stop its timers; fires onDown if it was ready. */
  private detachSocket(): void {
    this.socket = null;
    this.clearHandshake();
    if (this.pingTimer !== null) {
      clearInterval(this.pingTimer);
      this.pingTimer = null;
    }
    this.pingSentAt.clear();
    if (this.ready) {
      this.ready = false;
      this.events.onDown?.();
    }
  }

  private handleClosed(): void {
    if (this.retryOlderProtocol) {
      this.retryOlderProtocol = false;
      this.open();
      return;
    }
    const reason = this.lastError ?? { code: 'network' };
    if (isTerminal(reason)) {
      this.setState({ status: 'failed', reason });
      return;
    }
    this.failures += 1;
    const delay = backoffDelay(this.failures, this.opts.backoff, this.opts.random);
    this.setState({ status: 'reconnecting', attempt: this.failures, retryInMs: delay, reason });
    this.retryTimer = setTimeout(() => {
      this.retryTimer = null;
      this.open();
    }, delay);
  }

  private clearHandshake(): void {
    if (this.handshakeTimer !== null) {
      clearTimeout(this.handshakeTimer);
      this.handshakeTimer = null;
    }
  }

  private clearRetry(): void {
    if (this.retryTimer !== null) {
      clearTimeout(this.retryTimer);
      this.retryTimer = null;
    }
  }
}
