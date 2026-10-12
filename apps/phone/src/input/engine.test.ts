import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { InputEngine } from './engine';
import { WIIMOTE_BUTTONS, type InputMessage, type WiimoteButton } from '../protocol/types';
import { validateInput } from '../protocol/validate';

function makeEngine() {
  const engine = new InputEngine<WiimoteButton>(WIIMOTE_BUTTONS, { heartbeatMs: 50 });
  const sent: InputMessage[] = [];
  return { engine, sent, send: (m: InputMessage) => sent.push(m) };
}

describe('InputEngine state', () => {
  it('reports buttons in canonical order regardless of press order', () => {
    const { engine } = makeEngine();
    engine.press('p1', 'RIGHT');
    engine.press('p2', 'B');
    engine.press('p3', 'A');
    expect(engine.getButtons()).toEqual(['A', 'B', 'RIGHT']);
  });

  it('never produces duplicate buttons when several sources hold the same button', () => {
    const { engine, sent, send } = makeEngine();
    engine.attach(send);
    engine.press('p1', 'A');
    engine.press('p2', 'A');
    engine.press('p2', 'A');
    expect(engine.getButtons()).toEqual(['A']);
    for (const m of sent) expect(new Set(m.buttons).size).toBe(m.buttons.length);
    engine.detach();
  });

  it('keeps a button pressed until the last source releases it', () => {
    const { engine } = makeEngine();
    engine.press('p1', 'A');
    engine.press('p2', 'A');
    engine.releaseSource('p1');
    expect(engine.isPressed('A')).toBe(true);
    engine.releaseSource('p2');
    expect(engine.isPressed('A')).toBe(false);
  });

  it('setSource replaces what a source holds (D-pad slide)', () => {
    const { engine } = makeEngine();
    engine.setSource('dpad', ['UP']);
    engine.setSource('dpad', ['UP', 'RIGHT']);
    expect(engine.getButtons()).toEqual(['UP', 'RIGHT']);
    engine.setSource('dpad', ['RIGHT']);
    expect(engine.getButtons()).toEqual(['RIGHT']);
    engine.setSource('dpad', []);
    expect(engine.getButtons()).toEqual([]);
  });

  it('release() only drops one button of a source', () => {
    const { engine } = makeEngine();
    engine.setSource('p1', ['A', 'B']);
    expect(engine.release('p1', 'A')).toBe(true);
    expect(engine.getButtons()).toEqual(['B']);
    expect(engine.release('p1', 'A')).toBe(false);
  });

  it('releaseAll clears every source and notifies listeners once', () => {
    const { engine } = makeEngine();
    const listener = vi.fn();
    engine.press('p1', 'A');
    engine.press('p2', 'B');
    engine.setSource('dpad', ['UP', 'LEFT']);
    engine.subscribe(listener);
    expect(engine.releaseAll()).toBe(true);
    expect(engine.getButtons()).toEqual([]);
    expect(listener).toHaveBeenCalledTimes(1);
    expect(engine.releaseAll()).toBe(false);
  });

  it('keeps the same array instance while nothing changes', () => {
    const { engine } = makeEngine();
    engine.press('p1', 'A');
    const first = engine.getButtons();
    engine.press('p2', 'A');
    expect(engine.getButtons()).toBe(first);
  });

  it('ignores buttons that are not in the valid set', () => {
    const engine = new InputEngine<string>(WIIMOTE_BUTTONS);
    engine.press('p1', 'X');
    expect(engine.getButtons()).toEqual([]);
  });

  it('unsubscribe stops notifications', () => {
    const { engine } = makeEngine();
    const listener = vi.fn();
    const off = engine.subscribe(listener);
    off();
    engine.press('p1', 'A');
    expect(listener).not.toHaveBeenCalled();
  });
});

describe('InputEngine sending', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('sends a snapshot immediately on attach and on every change', () => {
    const { engine, sent, send } = makeEngine();
    engine.attach(send);
    expect(sent).toHaveLength(1);
    expect(sent[0]?.buttons).toEqual([]);
    engine.press('p1', 'A');
    expect(sent).toHaveLength(2);
    expect(sent[1]?.buttons).toEqual(['A']);
    engine.releaseSource('p1');
    expect(sent).toHaveLength(3);
    expect(sent[2]?.buttons).toEqual([]);
    engine.detach();
  });

  it('does not send when a press does not change the set', () => {
    const { engine, sent, send } = makeEngine();
    engine.press('p1', 'A');
    engine.attach(send);
    engine.press('p2', 'A');
    engine.press('p1', 'A');
    expect(sent).toHaveLength(1);
    engine.detach();
  });

  it('sends a heartbeat every 50 ms while attached', () => {
    const { engine, sent, send } = makeEngine();
    engine.attach(send);
    expect(sent).toHaveLength(1);
    vi.advanceTimersByTime(49);
    expect(sent).toHaveLength(1);
    vi.advanceTimersByTime(1);
    expect(sent).toHaveLength(2);
    vi.advanceTimersByTime(500);
    expect(sent).toHaveLength(12);
    engine.detach();
  });

  it('a change resets the heartbeat timer so the gap never exceeds 50 ms', () => {
    const { engine, sent, send } = makeEngine();
    const times: number[] = [];
    engine.attach((m) => {
      times.push(Date.now());
      send(m);
    });
    vi.advanceTimersByTime(30);
    engine.press('p1', 'A'); // t=30
    vi.advanceTimersByTime(49); // t=79: no heartbeat yet (last send at 30)
    expect(sent).toHaveLength(2);
    vi.advanceTimersByTime(1); // t=80
    expect(sent).toHaveLength(3);
    vi.advanceTimersByTime(1000);
    for (let i = 1; i < times.length; i++) {
      expect((times[i] ?? 0) - (times[i - 1] ?? 0)).toBeLessThanOrEqual(50);
    }
    engine.detach();
  });

  it('stops sending after detach', () => {
    const { engine, sent, send } = makeEngine();
    engine.attach(send);
    engine.detach();
    vi.advanceTimersByTime(1000);
    engine.press('p1', 'A');
    expect(sent).toHaveLength(1);
    expect(vi.getTimerCount()).toBe(0);
  });

  it('seq increases strictly across changes, heartbeats and re-attach', () => {
    const { engine, sent, send } = makeEngine();
    engine.attach(send);
    engine.press('p1', 'A');
    vi.advanceTimersByTime(120);
    engine.releaseAll();
    engine.detach();
    engine.attach(send);
    vi.advanceTimersByTime(120);
    engine.detach();
    expect(sent.length).toBeGreaterThan(6);
    for (let i = 1; i < sent.length; i++) {
      expect(sent[i]?.seq).toBeGreaterThan(sent[i - 1]?.seq ?? Infinity);
    }
    expect(engine.lastSeq).toBe(sent.at(-1)?.seq);
  });

  it('releaseAllAndFlush sends exactly one empty snapshot', () => {
    const { engine, sent, send } = makeEngine();
    engine.attach(send);
    engine.press('p1', 'A');
    engine.press('p2', 'B');
    const before = sent.length;
    engine.releaseAllAndFlush();
    expect(sent.length).toBe(before + 1);
    expect(sent.at(-1)?.buttons).toEqual([]);
    // Also when nothing was pressed.
    engine.releaseAllAndFlush();
    expect(sent.length).toBe(before + 2);
    expect(sent.at(-1)?.buttons).toEqual([]);
    engine.detach();
  });

  it('releaseAllAndFlush while detached clears state but sends nothing', () => {
    const { engine, sent } = makeEngine();
    engine.press('p1', 'A');
    engine.releaseAllAndFlush();
    expect(engine.getButtons()).toEqual([]);
    expect(sent).toHaveLength(0);
  });

  it('every snapshot is valid according to the protocol validator', () => {
    const { engine, sent, send } = makeEngine();
    engine.attach(send);
    for (const b of WIIMOTE_BUTTONS) engine.press(`p-${b}`, b);
    vi.advanceTimersByTime(200);
    engine.releaseAll();
    engine.detach();
    expect(sent.some((m) => m.buttons.length === WIIMOTE_BUTTONS.length)).toBe(true);
    for (const m of sent) {
      expect(validateInput(m, 'wiimote')).toEqual([]);
      expect(m.pointer).toBeNull();
      expect('motion' in m).toBe(false);
    }
  });
});
