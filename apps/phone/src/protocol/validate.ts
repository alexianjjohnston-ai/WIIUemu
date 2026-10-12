// A small validator that mirrors the server-side rules in docs/controller-protocol.md.
// Used by unit tests and by the e2e mock server to prove every frame the phone sends is valid.

import {
  GAMEPAD_BUTTONS,
  MAX_BUTTONS,
  MAX_FRAME_BYTES,
  PROTOCOL_VERSION,
  WIIMOTE_BUTTONS,
  type Role,
  type ServerMessage,
} from './types';

const MAX_SEQ = 2 ** 53; // exclusive
const DEVICE_ID_RE = /^[A-Za-z0-9-]{8,64}$/;
const MAX_NAME_CHARS = 40;

type Obj = Record<string, unknown>;

function isObject(v: unknown): v is Obj {
  return typeof v === 'object' && v !== null && !Array.isArray(v);
}

function isFiniteNumber(v: unknown): v is number {
  return typeof v === 'number' && Number.isFinite(v);
}

function inRange(v: unknown, min: number, max: number): boolean {
  return isFiniteNumber(v) && v >= min && v <= max;
}

function checkKeys(obj: Obj, allowed: readonly string[], errors: string[], where: string): void {
  for (const key of Object.keys(obj)) {
    if (!allowed.includes(key)) errors.push(`${where}: unexpected field "${key}"`);
  }
}

function checkVec2(v: unknown, min: number, max: number, errors: string[], where: string): void {
  if (!isObject(v)) {
    errors.push(`${where} must be an object`);
    return;
  }
  checkKeys(v, ['x', 'y'], errors, where);
  if (!inRange(v.x, min, max)) errors.push(`${where}.x must be a finite number in [${min},${max}]`);
  if (!inRange(v.y, min, max)) errors.push(`${where}.y must be a finite number in [${min},${max}]`);
}

function checkTriple(v: unknown, errors: string[], where: string): void {
  if (!Array.isArray(v) || v.length !== 3 || !v.every(isFiniteNumber)) {
    errors.push(`${where} must be an array of 3 finite numbers`);
  }
}

/** UTF-8 byte length of a string. */
export function byteLength(text: string): number {
  return new TextEncoder().encode(text).length;
}

/** Errors for a serialized frame as a whole (size limit). */
export function validateFrame(text: string): string[] {
  const size = byteLength(text);
  return size > MAX_FRAME_BYTES ? [`frame is ${size} bytes, limit is ${MAX_FRAME_BYTES}`] : [];
}

export function validateHello(msg: unknown): string[] {
  const errors: string[] = [];
  if (!isObject(msg)) return ['hello must be an object'];
  checkKeys(msg, ['type', 'protocol', 'token', 'role', 'deviceId', 'name'], errors, 'hello');
  if (msg.type !== 'hello') errors.push('type must be "hello"');
  if (msg.protocol !== PROTOCOL_VERSION) errors.push(`protocol must equal ${PROTOCOL_VERSION}`);
  if (typeof msg.token !== 'string' || msg.token.length === 0) errors.push('token must be a non-empty string');
  if (msg.role !== 'wiimote' && msg.role !== 'gamepad') errors.push('role must be "wiimote" or "gamepad"');
  if (typeof msg.deviceId !== 'string' || !DEVICE_ID_RE.test(msg.deviceId)) {
    errors.push('deviceId must be 8-64 chars of [A-Za-z0-9-]');
  }
  if ('name' in msg) {
    if (typeof msg.name !== 'string' || [...msg.name].length > MAX_NAME_CHARS) {
      errors.push(`name must be a string of at most ${MAX_NAME_CHARS} chars`);
    }
  }
  return errors;
}

export function validateInput(msg: unknown, role: Role): string[] {
  const errors: string[] = [];
  if (!isObject(msg)) return ['input must be an object'];
  checkKeys(msg, ['type', 'seq', 'buttons', 'pointer', 'stick', 'rstick', 'motion'], errors, 'input');
  if (msg.type !== 'input') errors.push('type must be "input"');

  if (!Number.isSafeInteger(msg.seq) || (msg.seq as number) < 0 || (msg.seq as number) >= MAX_SEQ) {
    errors.push('seq must be an integer with 0 <= seq < 2^53');
  }

  const valid: readonly string[] = role === 'wiimote' ? WIIMOTE_BUTTONS : GAMEPAD_BUTTONS;
  if (!Array.isArray(msg.buttons)) {
    errors.push('buttons must be an array');
  } else {
    if (msg.buttons.length > MAX_BUTTONS) errors.push(`buttons has more than ${MAX_BUTTONS} entries`);
    const seen = new Set<unknown>();
    for (const b of msg.buttons) {
      if (typeof b !== 'string' || !valid.includes(b)) errors.push(`invalid ${role} button ${JSON.stringify(b)}`);
      if (seen.has(b)) errors.push(`duplicate button ${JSON.stringify(b)}`);
      seen.add(b);
    }
  }

  if (!('pointer' in msg)) {
    errors.push('pointer is required (object or null)');
  } else if (msg.pointer !== null) {
    checkVec2(msg.pointer, 0, 1, errors, 'pointer');
  }

  if ('stick' in msg) checkVec2(msg.stick, -1, 1, errors, 'stick');
  if ('rstick' in msg) {
    if (role !== 'gamepad') errors.push('rstick is only valid for the gamepad role');
    checkVec2(msg.rstick, -1, 1, errors, 'rstick');
  }

  if ('motion' in msg && msg.motion !== null) {
    const m = msg.motion;
    if (!isObject(m)) {
      errors.push('motion must be an object or null');
    } else {
      checkKeys(m, ['accel', 'gyro', 'ts'], errors, 'motion');
      checkTriple(m.accel, errors, 'motion.accel');
      checkTriple(m.gyro, errors, 'motion.gyro');
      if (!isFiniteNumber(m.ts)) errors.push('motion.ts must be a finite number');
    }
  }
  return errors;
}

export function validatePing(msg: unknown): string[] {
  if (!isObject(msg)) return ['ping must be an object'];
  const errors: string[] = [];
  checkKeys(msg, ['type', 'id'], errors, 'ping');
  if (msg.type !== 'ping') errors.push('type must be "ping"');
  if (!isFiniteNumber(msg.id)) errors.push('id must be a finite number');
  return errors;
}

/**
 * Validates any client frame (as sent on the wire). Returns a list of errors; empty means valid.
 * `role` is needed to check `input.buttons`.
 */
export function validateClientFrame(text: string, role: Role): string[] {
  const errors = validateFrame(text);
  let msg: unknown;
  try {
    msg = JSON.parse(text);
  } catch {
    return [...errors, 'not valid JSON'];
  }
  const type = isObject(msg) ? msg.type : undefined;
  switch (type) {
    case 'hello':
      return [...errors, ...validateHello(msg)];
    case 'input':
      return [...errors, ...validateInput(msg, role)];
    case 'ping':
      return [...errors, ...validatePing(msg)];
    default:
      return [...errors, `unknown type ${JSON.stringify(type)}`];
  }
}

/** Parses a server frame. Returns null for anything that is not a well-formed known message. */
export function parseServerMessage(text: string): ServerMessage | null {
  let msg: unknown;
  try {
    msg = JSON.parse(text);
  } catch {
    return null;
  }
  if (!isObject(msg)) return null;
  switch (msg.type) {
    case 'welcome':
      if (
        isFiniteNumber(msg.protocol) &&
        (msg.role === 'wiimote' || msg.role === 'gamepad') &&
        Number.isInteger(msg.slot) &&
        Number.isInteger(msg.player)
      ) {
        return {
          type: 'welcome',
          protocol: msg.protocol,
          role: msg.role,
          slot: msg.slot as number,
          player: msg.player as number,
        };
      }
      return null;
    case 'pong':
      return isFiniteNumber(msg.id) ? { type: 'pong', id: msg.id } : null;
    case 'error':
      return typeof msg.code === 'string'
        ? { type: 'error', code: msg.code, message: typeof msg.message === 'string' ? msg.message : '' }
        : null;
    default:
      return null;
  }
}
