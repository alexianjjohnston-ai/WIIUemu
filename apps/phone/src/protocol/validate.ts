// A small validator that mirrors the server-side rules in docs/controller-protocol.md.
// Used by unit tests and by the e2e mock server to prove every frame the phone sends is valid.

import {
  CAMERA_FRAME_HEADER_BYTES,
  CAMERA_HEIGHT_RANGE,
  CAMERA_WIDTH_RANGE,
  GAMEPAD_BUTTONS,
  MAX_BUTTONS,
  MAX_MOTION_ACCEL_G,
  MAX_MOTION_GYRO_DPS,
  MAX_CAMERA_PAYLOAD_BYTES,
  MAX_FRAME_BYTES,
  MAX_GYRO_RAD_S,
  MAX_GYRO_SAMPLES,
  MIN_PROTOCOL_VERSION,
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

function checkTriple(v: unknown, errors: string[], where: string, max: number): void {
  if (!Array.isArray(v) || v.length !== 3 || !v.every(isFiniteNumber)) {
    errors.push(`${where} must be an array of 3 finite numbers`);
  } else if (v.some((n) => Math.abs(n) > max)) {
    errors.push(`${where} values must be within ±${max}`);
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
  if (
    !Number.isInteger(msg.protocol) ||
    (msg.protocol as number) < MIN_PROTOCOL_VERSION ||
    (msg.protocol as number) > PROTOCOL_VERSION
  ) {
    errors.push(`protocol must be an integer from ${MIN_PROTOCOL_VERSION} to ${PROTOCOL_VERSION}`);
  }
  if (typeof msg.token !== 'string' || msg.token.length === 0) errors.push('token must be a non-empty string');
  if (msg.role !== 'wiimote' && msg.role !== 'gamepad') errors.push('role must be "wiimote" or "gamepad"');
  if (typeof msg.deviceId !== 'string' || !DEVICE_ID_RE.test(msg.deviceId)) {
    errors.push('deviceId must be 8-64 chars of [A-Za-z0-9-]');
  }
  if ('name' in msg) {
    if (typeof msg.name !== 'string' || Array.from(msg.name).length > MAX_NAME_CHARS) {
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
      checkTriple(m.accel, errors, 'motion.accel', MAX_MOTION_ACCEL_G);
      checkTriple(m.gyro, errors, 'motion.gyro', MAX_MOTION_GYRO_DPS);
      if (!Number.isSafeInteger(m.ts) || (m.ts as number) < 0) {
        errors.push('motion.ts must be an integer with 0 <= ts < 2^53');
      }
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

/** Protocol 2 `gyro`. `lastT` is the last timestamp sent earlier on the same connection. */
export function validateGyro(msg: unknown, lastT: number | null = null): string[] {
  if (!isObject(msg)) return ['gyro must be an object'];
  const errors: string[] = [];
  checkKeys(msg, ['type', 'samples'], errors, 'gyro');
  if (msg.type !== 'gyro') errors.push('type must be "gyro"');
  if (!Array.isArray(msg.samples) || msg.samples.length < 1 || msg.samples.length > MAX_GYRO_SAMPLES) {
    return [...errors, `samples must be an array of 1-${MAX_GYRO_SAMPLES} entries`];
  }
  let prev = lastT;
  const samples: unknown[] = msg.samples;
  for (const [i, sample] of samples.entries()) {
    if (!Array.isArray(sample) || sample.length !== 4 || !sample.every(isFiniteNumber)) {
      errors.push(`samples[${i}] must be [t_us, x, y, z] of finite numbers`);
      continue;
    }
    const [t, ...rates] = sample;
    if (t === undefined || !Number.isSafeInteger(t) || t < 0) errors.push(`samples[${i}] t_us must be an integer >= 0`);
    else if (prev !== null && t <= prev) errors.push(`samples[${i}] t_us must strictly increase`);
    else prev = t;
    if (rates.some((r) => Math.abs(r) > MAX_GYRO_RAD_S))
      errors.push(`samples[${i}] rate exceeds ${MAX_GYRO_RAD_S} rad/s`);
  }
  return errors;
}

/** Protocol 2 binary `camera_frame`: checks the header and the payload size (not the JPEG itself). */
export function validateCameraFrame(bytes: Uint8Array, lastSeq: number | null = null): string[] {
  if (bytes.length < CAMERA_FRAME_HEADER_BYTES) return ['camera frame shorter than its header'];
  const errors: string[] = [];
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  if (view.getUint8(0) !== 1) errors.push('kind must be 1');
  if (view.getUint8(1) !== 1) errors.push('format must be 1 (JPEG)');
  const w = view.getUint16(2, true);
  const h = view.getUint16(4, true);
  if (w < CAMERA_WIDTH_RANGE[0] || w > CAMERA_WIDTH_RANGE[1]) errors.push(`width ${w} out of range`);
  if (h < CAMERA_HEIGHT_RANGE[0] || h > CAMERA_HEIGHT_RANGE[1]) errors.push(`height ${h} out of range`);
  if (view.getUint16(6, true) !== 0) errors.push('reserved must be 0');
  const seq = view.getUint32(8, true);
  if (lastSeq !== null && seq <= lastSeq) errors.push('seq must strictly increase');
  if (view.getBigUint64(12, true) > BigInt(Number.MAX_SAFE_INTEGER)) errors.push('capture_us exceeds 2^53');
  const payload = bytes.length - CAMERA_FRAME_HEADER_BYTES;
  if (payload === 0 || payload > MAX_CAMERA_PAYLOAD_BYTES) errors.push(`payload is ${payload} bytes`);
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
    case 'gyro':
      return [...errors, ...validateGyro(msg)];
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
        ? {
            type: 'error',
            code: msg.code,
            message: typeof msg.message === 'string' ? msg.message : '',
          }
        : null;
    case 'tracking': {
      const p = msg.pointer;
      // The tracked aim point may lie outside [0,1] when the phone points beside the TV.
      const pointer = isObject(p) && isFiniteNumber(p.x) && isFiniteNumber(p.y) ? { x: p.x, y: p.y } : null;
      if (
        (msg.state === 'tracking' || msg.state === 'gyro_only' || msg.state === 'lost') &&
        (p === null || pointer !== null) &&
        Number.isInteger(msg.tags) &&
        isFiniteNumber(msg.fixes_per_s)
      ) {
        return {
          type: 'tracking',
          state: msg.state,
          pointer,
          tags: msg.tags as number,
          fixes_per_s: msg.fixes_per_s,
        };
      }
      return null;
    }
    default:
      return null;
  }
}
