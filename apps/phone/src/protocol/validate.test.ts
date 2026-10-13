import { describe, expect, it } from 'vitest';
import {
  parseServerMessage,
  validateClientFrame,
  validateFrame,
  validateHello,
  validateInput,
} from './validate';

const okInput = { type: 'input', seq: 1, buttons: ['A', 'B'], pointer: null };

describe('validateInput', () => {
  it('accepts a minimal Wii Remote snapshot', () => {
    expect(validateInput(okInput, 'wiimote')).toEqual([]);
  });

  it('accepts the full example from the spec', () => {
    const msg = {
      type: 'input',
      seq: 1042,
      buttons: ['A', 'B'],
      pointer: { x: 0.512, y: 0.43 },
      stick: { x: 0, y: 0 },
      motion: { accel: [0, -1, 0], gyro: [0, 0, 0], ts: 1234567 },
    };
    expect(validateInput(msg, 'wiimote')).toEqual([]);
  });

  it('accepts motion: null', () => {
    expect(validateInput({ ...okInput, motion: null }, 'wiimote')).toEqual([]);
  });

  it.each([
    ['negative seq', { ...okInput, seq: -1 }],
    ['fractional seq', { ...okInput, seq: 1.5 }],
    ['seq >= 2^53', { ...okInput, seq: 2 ** 53 }],
    ['string seq', { ...okInput, seq: '1' }],
    ['duplicate buttons', { ...okInput, buttons: ['A', 'A'] }],
    ['unknown button', { ...okInput, buttons: ['X'] }],
    ['gamepad-only button', { ...okInput, buttons: ['ZL'] }],
    ['too many buttons', { ...okInput, buttons: Array.from({ length: 17 }, () => 'A') }],
    ['missing pointer', { type: 'input', seq: 1, buttons: [] }],
    ['pointer out of range', { ...okInput, pointer: { x: 1.1, y: 0 } }],
    ['pointer NaN', { ...okInput, pointer: { x: NaN, y: 0 } }],
    ['stick out of range', { ...okInput, stick: { x: 0, y: -1.5 } }],
    ['rstick on wiimote', { ...okInput, rstick: { x: 0, y: 0 } }],
    ['bad motion', { ...okInput, motion: { accel: [0, 0], gyro: [0, 0, 0], ts: 1 } }],
    ['infinite motion', { ...okInput, motion: { accel: [0, 0, Infinity], gyro: [0, 0, 0], ts: 1 } }],
    ['unexpected field', { ...okInput, extra: 1 }],
    ['wrong type', { ...okInput, type: 'hello' }],
  ])('rejects %s', (_name, msg) => {
    expect(validateInput(msg, 'wiimote').length).toBeGreaterThan(0);
  });

  it('accepts GamePad buttons and rstick for the gamepad role', () => {
    expect(
      validateInput({ ...okInput, buttons: ['ZL', 'X', 'L3'], rstick: { x: 1, y: -1 } }, 'gamepad'),
    ).toEqual([]);
  });

  it('rejects Nunchuk buttons for the gamepad role', () => {
    expect(validateInput({ ...okInput, buttons: ['C'] }, 'gamepad').length).toBeGreaterThan(0);
  });
});

describe('validateHello', () => {
  const okHello = {
    type: 'hello',
    protocol: 1,
    token: 'k3J9abc',
    role: 'wiimote',
    deviceId: '6f1c2a4e-1111-4222-8333-444455556666',
  };

  it('accepts a valid hello with and without name', () => {
    expect(validateHello(okHello)).toEqual([]);
    expect(validateHello({ ...okHello, name: 'Pixel 8' })).toEqual([]);
  });

  it.each([
    ['protocol 3', { ...okHello, protocol: 3 }],
    ['protocol 0', { ...okHello, protocol: 0 }],
    ['empty token', { ...okHello, token: '' }],
    ['bad role', { ...okHello, role: 'nunchuk' }],
    ['short deviceId', { ...okHello, deviceId: 'abc' }],
    ['deviceId with bad chars', { ...okHello, deviceId: 'abc_def_ghi' }],
    ['deviceId too long', { ...okHello, deviceId: 'a'.repeat(65) }],
    ['name too long', { ...okHello, name: 'x'.repeat(41) }],
  ])('rejects %s', (_name, msg) => {
    expect(validateHello(msg).length).toBeGreaterThan(0);
  });
});

describe('validateClientFrame', () => {
  it('rejects frames over 4 KiB', () => {
    expect(validateFrame('x'.repeat(4097)).length).toBe(1);
    expect(validateFrame('x'.repeat(4096))).toEqual([]);
  });

  it('dispatches on type', () => {
    expect(validateClientFrame(JSON.stringify(okInput), 'wiimote')).toEqual([]);
    expect(validateClientFrame('{"type":"ping","id":7}', 'wiimote')).toEqual([]);
    expect(validateClientFrame('{"type":"nope"}', 'wiimote')).toEqual(['unknown type "nope"']);
    expect(validateClientFrame('not json', 'wiimote')).toEqual(['not valid JSON']);
  });
});

describe('parseServerMessage', () => {
  it('parses welcome, pong and error', () => {
    expect(parseServerMessage('{"type":"welcome","protocol":1,"role":"wiimote","slot":1,"player":1}')).toEqual({
      type: 'welcome',
      protocol: 1,
      role: 'wiimote',
      slot: 1,
      player: 1,
    });
    expect(parseServerMessage('{"type":"pong","id":7}')).toEqual({ type: 'pong', id: 7 });
    expect(parseServerMessage('{"type":"error","code":"bad_token","message":"nope"}')).toEqual({
      type: 'error',
      code: 'bad_token',
      message: 'nope',
    });
  });

  it('returns null for malformed or unknown messages', () => {
    expect(parseServerMessage('garbage')).toBeNull();
    expect(parseServerMessage('[]')).toBeNull();
    expect(parseServerMessage('{"type":"welcome","slot":"1"}')).toBeNull();
    expect(parseServerMessage('{"type":"surprise"}')).toBeNull();
  });

  it('reads where the tags are from tracking messages, and ignores values it does not know', () => {
    const base = '"type":"tracking","state":"tracking","pointer":null,"tags":9,"fixes_per_s":29';
    expect(parseServerMessage(`{${base},"border":"game"}`)).toMatchObject({ border: 'game' });
    expect(parseServerMessage(`{${base},"border":"page"}`)).toMatchObject({ border: 'page' });
    // Older servers leave it out; newer ones may add places this phone doesn't know. Both mean "page".
    expect(parseServerMessage(`{${base}}`)).not.toHaveProperty('border');
    expect(parseServerMessage(`{${base},"border":"window"}`)).not.toHaveProperty('border');
    expect(parseServerMessage(`{${base},"border":7}`)).not.toHaveProperty('border');
  });
});
