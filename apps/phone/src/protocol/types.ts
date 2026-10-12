// Types for the phone <-> PC WebSocket protocol (version 1).
// Source of truth: docs/controller-protocol.md. Do not change shapes here without changing the spec.

export const PROTOCOL_VERSION = 1;

/** Server closes the connection on frames larger than this (bytes). */
export const MAX_FRAME_BYTES = 4096;

/** At most this many entries in `input.buttons`. */
export const MAX_BUTTONS = 16;

export type Role = 'wiimote' | 'gamepad';

/** Wii Remote buttons, in the canonical order the client sends them. */
export const WIIMOTE_BUTTONS = [
  'A',
  'B',
  'ONE',
  'TWO',
  'PLUS',
  'MINUS',
  'HOME',
  'UP',
  'DOWN',
  'LEFT',
  'RIGHT',
  'C',
  'Z',
] as const;
export type WiimoteButton = (typeof WIIMOTE_BUTTONS)[number];

export const GAMEPAD_BUTTONS = [
  'A',
  'B',
  'X',
  'Y',
  'L',
  'R',
  'ZL',
  'ZR',
  'PLUS',
  'MINUS',
  'HOME',
  'UP',
  'DOWN',
  'LEFT',
  'RIGHT',
  'L3',
  'R3',
] as const;
export type GamepadButton = (typeof GAMEPAD_BUTTONS)[number];

export interface Vec2 {
  x: number;
  y: number;
}

export interface Motion {
  accel: [number, number, number];
  gyro: [number, number, number];
  ts: number;
}

export interface HelloMessage {
  type: 'hello';
  protocol: typeof PROTOCOL_VERSION;
  token: string;
  role: Role;
  deviceId: string;
  name?: string;
}

export interface InputMessage {
  type: 'input';
  seq: number;
  buttons: string[];
  pointer: Vec2 | null;
  stick?: Vec2;
  rstick?: Vec2;
  motion?: Motion | null;
}

export interface PingMessage {
  type: 'ping';
  id: number;
}

export type ClientMessage = HelloMessage | InputMessage | PingMessage;

export const ERROR_CODES = [
  'bad_token',
  'unsupported_protocol',
  'invalid_message',
  'role_unavailable',
  'timeout',
  'replaced',
] as const;
export type ErrorCode = (typeof ERROR_CODES)[number];

export interface WelcomeMessage {
  type: 'welcome';
  protocol: number;
  role: Role;
  slot: number;
  player: number;
}

export interface PongMessage {
  type: 'pong';
  id: number;
}

export interface ErrorMessage {
  type: 'error';
  /** Usually one of ERROR_CODES; kept as string so newer servers do not break older phones. */
  code: string;
  message: string;
}

export type ServerMessage = WelcomeMessage | PongMessage | ErrorMessage;
