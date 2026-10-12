import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { Connection, type SocketLike } from './connection';
import { backoffDelay } from './backoff';
import { describeError, isTerminal } from './errors';
import { validateClientFrame } from '../protocol/validate';
import { ERROR_CODES } from '../protocol/types';

class FakeSocket implements SocketLike {
  readyState = 0;
  bufferedAmount = 0;
  onopen: ((ev: Event) => void) | null = null;
  onmessage: ((ev: MessageEvent) => void) | null = null;
  onclose: ((ev: CloseEvent) => void) | null = null;
  onerror: ((ev: Event) => void) | null = null;
  sent: string[] = [];
  binary: Uint8Array[] = [];
  closed: { code: number | undefined } | null = null;
  constructor(readonly url: string) {}

  send(data: string | Uint8Array) {
    if (typeof data === 'string') this.sent.push(data);
    else this.binary.push(data);
  }
  close(code?: number) {
    this.closed = { code };
    this.readyState = 3;
  }
  // test helpers
  serverOpen() {
    this.readyState = 1;
    this.onopen?.(new Event('open'));
  }
  serverSend(msg: object) {
    this.onmessage?.({ data: JSON.stringify(msg) } as MessageEvent);
  }
  serverClose() {
    this.readyState = 3;
    this.onclose?.({ code: 1006 } as CloseEvent);
  }
  messages() {
    return this.sent.map((s) => JSON.parse(s) as Record<string, unknown>);
  }
}

const WELCOME = { type: 'welcome', protocol: 2, role: 'wiimote', slot: 1, player: 1 };

function setup() {
  const sockets: FakeSocket[] = [];
  const events = { onReady: vi.fn(), onDown: vi.fn(), onTracking: vi.fn() };
  const conn = new Connection(
    {
      url: 'ws://pc/ws',
      hello: { token: 'tok123', role: 'wiimote', deviceId: 'device-0001' },
      createSocket: (url) => {
        const s = new FakeSocket(url);
        sockets.push(s);
        return s;
      },
      random: () => 0.5, // no jitter
      now: () => Date.now(),
    },
    events,
  );
  const last = () => {
    const s = sockets.at(-1);
    if (!s) throw new Error('no socket');
    return s;
  };
  return { conn, sockets, events, last };
}

describe('Connection', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('sends a valid hello on open and becomes connected on welcome', () => {
    const { conn, last, events } = setup();
    conn.start();
    expect(conn.getState()).toEqual({ status: 'connecting', attempt: 1 });
    last().serverOpen();
    expect(last().messages()[0]).toEqual({
      type: 'hello',
      protocol: 2,
      token: 'tok123',
      role: 'wiimote',
      deviceId: 'device-0001',
    });
    expect(validateClientFrame(last().sent[0] ?? '', 'wiimote')).toEqual([]);
    last().serverSend(WELCOME);
    const state = conn.getState();
    expect(state.status).toBe('connected');
    expect(state.status === 'connected' && state.welcome.player).toBe(1);
    expect(events.onReady).toHaveBeenCalledTimes(1);
    expect(conn.isReady).toBe(true);
    conn.stop();
  });

  it('reconnects with exponential backoff after an unexpected close', () => {
    const { conn, sockets, last, events } = setup();
    conn.start();
    last().serverOpen();
    last().serverSend(WELCOME);
    last().serverClose();
    expect(events.onDown).toHaveBeenCalledTimes(1);
    let state = conn.getState();
    expect(state).toMatchObject({ status: 'reconnecting', attempt: 1, retryInMs: 500, reason: { code: 'network' } });

    vi.advanceTimersByTime(499);
    expect(sockets).toHaveLength(1);
    vi.advanceTimersByTime(1);
    expect(sockets).toHaveLength(2);
    // Second failure before welcome doubles the delay.
    last().serverClose();
    state = conn.getState();
    expect(state).toMatchObject({ status: 'reconnecting', attempt: 2, retryInMs: 1000 });
    vi.advanceTimersByTime(1000);
    expect(sockets).toHaveLength(3);
    // Welcome resets the failure counter.
    last().serverOpen();
    last().serverSend(WELCOME);
    last().serverClose();
    expect(conn.getState()).toMatchObject({ status: 'reconnecting', attempt: 1, retryInMs: 500 });
    conn.stop();
    expect(conn.getState()).toEqual({ status: 'idle' });
    vi.advanceTimersByTime(10_000);
    expect(sockets).toHaveLength(3);
  });

  it.each(['bad_token', 'unsupported_protocol', 'role_unavailable', 'replaced', 'invalid_message'])(
    'stops reconnecting after error %s',
    (code) => {
      const { conn, sockets, last } = setup();
      conn.start();
      last().serverOpen();
      last().serverSend({ type: 'error', code, message: 'nope' });
      last().serverClose();
      expect(conn.getState()).toEqual({ status: 'failed', reason: { code, detail: 'nope' } });
      vi.advanceTimersByTime(60_000);
      expect(sockets).toHaveLength(1);
    },
  );

  it('keeps retrying after a timeout error', () => {
    const { conn, sockets, last } = setup();
    conn.start();
    last().serverOpen();
    last().serverSend({ type: 'error', code: 'timeout', message: 'no hello' });
    last().serverClose();
    expect(conn.getState()).toMatchObject({ status: 'reconnecting', reason: { code: 'timeout' } });
    vi.advanceTimersByTime(500);
    expect(sockets).toHaveLength(2);
    conn.stop();
  });

  it('start() after failure tries again', () => {
    const { conn, sockets, last } = setup();
    conn.start();
    last().serverSend({ type: 'error', code: 'bad_token', message: '' });
    last().serverClose();
    conn.start();
    expect(sockets).toHaveLength(2);
    expect(conn.getState()).toEqual({ status: 'connecting', attempt: 1 });
    conn.stop();
  });

  it('gives up on an attempt when no welcome arrives in time', () => {
    const { conn, sockets, last } = setup();
    conn.start();
    last().serverOpen();
    vi.advanceTimersByTime(8000);
    expect(last().closed).not.toBeNull();
    expect(conn.getState()).toMatchObject({ status: 'reconnecting', reason: { code: 'handshake_timeout' } });
    vi.advanceTimersByTime(500);
    expect(sockets).toHaveLength(2);
    conn.stop();
  });

  it('pings while connected, measures RTT and drops a stalled link', () => {
    const { conn, last, events } = setup();
    conn.start();
    last().serverOpen();
    last().serverSend(WELCOME);
    vi.advanceTimersByTime(2000);
    const ping = last().messages().find((m) => m.type === 'ping');
    expect(ping).toEqual({ type: 'ping', id: 1 });
    vi.advanceTimersByTime(30);
    last().serverSend({ type: 'pong', id: 1 });
    expect(conn.getState()).toMatchObject({ status: 'connected', rttMs: 30 });
    // Silence from the server: after > 6 s the socket is abandoned.
    vi.advanceTimersByTime(8000);
    expect(events.onDown).toHaveBeenCalledTimes(1);
    expect(conn.getState()).toMatchObject({ status: 'reconnecting', reason: { code: 'stale' } });
    conn.stop();
  });

  it('retryNow skips the backoff wait', () => {
    const { conn, sockets, last } = setup();
    conn.start();
    last().serverClose();
    expect(conn.getState().status).toBe('reconnecting');
    conn.retryNow();
    expect(sockets).toHaveLength(2);
    conn.stop();
  });

  it('send() refuses when the socket is not open and skips input when congested', () => {
    const { conn, last } = setup();
    conn.start();
    expect(conn.send({ type: 'ping', id: 1 })).toBe(false);
    last().serverOpen();
    last().bufferedAmount = 1 << 20;
    expect(conn.send({ type: 'input', seq: 1, buttons: [], pointer: null })).toBe(false);
    expect(conn.send({ type: 'ping', id: 1 })).toBe(true);
    conn.stop();
  });

  it('protocol 2: sends frames only when the socket is drained, and forwards tracking feedback', () => {
    const { conn, last, events } = setup();
    conn.start();
    last().serverOpen();
    const tracking = { type: 'tracking', state: 'tracking', pointer: { x: 0.5, y: 0.5 }, tags: 7, fixes_per_s: 30 };
    last().serverSend(tracking);
    expect(events.onTracking).not.toHaveBeenCalled(); // not before welcome
    last().serverSend(WELCOME);
    expect(conn.sendFrame(new Uint8Array(30))).toBe(true);
    expect(last().binary).toHaveLength(1);
    last().bufferedAmount = 40_000;
    expect(conn.sendFrame(new Uint8Array(30))).toBe(false);
    expect(conn.send({ type: 'input', seq: 1, buttons: ['A'], pointer: null })).toBe(true); // input still goes
    expect(conn.send({ type: 'gyro', samples: [[1, 0, 0, 0]] })).toBe(true);
    last().serverSend(tracking);
    expect(events.onTracking).toHaveBeenCalledWith(tracking);
  });

  it('accepts a protocol 1 server but keeps camera tracking off', () => {
    const { conn, last } = setup();
    conn.start();
    last().serverOpen();
    last().serverSend({ ...WELCOME, protocol: 1 });
    expect(conn.getState()).toMatchObject({ status: 'connected' });
    expect(conn.supportsTracking).toBe(false);
    expect(conn.canSendFrame()).toBe(false);
    expect(conn.sendFrame(new Uint8Array(30))).toBe(false);
    expect(conn.send({ type: 'gyro', samples: [[1, 0, 0, 0]] })).toBe(false);
  });

  it('closes when the server announces another protocol version', () => {
    const { conn, last } = setup();
    conn.start();
    last().serverOpen();
    last().serverSend({ ...WELCOME, protocol: 3 });
    expect(conn.getState()).toMatchObject({ status: 'failed', reason: { code: 'unsupported_protocol' } });
  });
});

describe('backoffDelay', () => {
  it('doubles up to the cap', () => {
    const r = () => 0.5;
    expect([1, 2, 3, 4, 5, 6, 7].map((a) => backoffDelay(a, undefined, r))).toEqual([
      500, 1000, 2000, 4000, 8000, 8000, 8000,
    ]);
  });
  it('applies +-20% jitter', () => {
    expect(backoffDelay(1, undefined, () => 0)).toBe(400);
    expect(backoffDelay(1, undefined, () => 0.999999)).toBe(600);
  });
});

describe('describeError', () => {
  it('has a readable title and hint for every protocol error code', () => {
    for (const code of ERROR_CODES) {
      const d = describeError({ code });
      expect(d.title.length).toBeGreaterThan(5);
      expect(d.hint.length).toBeGreaterThan(5);
    }
  });
  it('treats unknown codes as retryable', () => {
    expect(isTerminal({ code: 'brand_new_code' })).toBe(false);
    expect(describeError({ code: 'brand_new_code', detail: 'x' }).hint).toContain('x');
  });
});
