// Types for the phone <-> PC WebSocket protocol (version 2 = version 1 + camera tracking).
// Source of truth: docs/controller-protocol.md. Do not change shapes here without changing the spec.

/** Version this app sends in `hello`. */
export const PROTOCOL_VERSION = 2;
/** Oldest version the app still accepts in `welcome` (camera tracking is off on version 1). */
export const MIN_PROTOCOL_VERSION = 1;

/** Server closes the connection on frames larger than this (bytes). */
export const MAX_FRAME_BYTES = 4096;

/** At most this many entries in `input.buttons`. */
export const MAX_BUTTONS = 16;

/** `input.motion` limits (the server rejects anything outside them). */
export const MAX_MOTION_ACCEL_G = 64;
export const MAX_MOTION_GYRO_DPS = 8000;

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
  protocol: number;
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

/** Protocol 2: gyro batch. Each sample is `[t_us, x, y, z]`, rates in rad/s in the W3C device frame. */
export interface GyroMessage {
  type: 'gyro';
  samples: [number, number, number, number][];
}

/** Protocol 2: the player is aiming at the centre of the TV now; the PC measures the aim point. */
export interface CalibrateMessage {
  type: 'calibrate';
}

/** Protocol 2: apply a stored aim point (fraction of the camera image), or the image centre for `null`. */
export interface AimMessage {
  type: 'aim';
  point: Vec2 | null;
}

export type ClientMessage = HelloMessage | InputMessage | PingMessage | GyroMessage | CalibrateMessage | AimMessage;

/** Protocol 2 limits for `gyro` and binary `camera_frame` messages. */
export const MAX_GYRO_SAMPLES = 64;
export const MAX_GYRO_RAD_S = 35;
export const CAMERA_FRAME_HEADER_BYTES = 20;
export const MAX_CAMERA_PAYLOAD_BYTES = 65_536;
export const CAMERA_WIDTH_RANGE = [160, 1280] as const;
export const CAMERA_HEIGHT_RANGE = [120, 960] as const;

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

export type TrackingState = 'tracking' | 'gyro_only' | 'lost';

/** Protocol 2: camera-tracking feedback, about 10 Hz while frames arrive. */
export interface TrackingMessage {
  type: 'tracking';
  state: TrackingState;
  pointer: Vec2 | null;
  tags: number;
  fixes_per_s: number;
  /** Where the tags are: around Cemu's game (`game`) or on the PC's /tracking page. Missing means `page`. */
  border?: 'game' | 'page';
}

/** Why a calibration failed; kept as string so newer servers do not break older phones. */
export type CalibrationFailure = 'no_markers' | 'off_image' | (string & {});

/** Protocol 2: result of `calibrate`. */
export type CalibrationMessage =
  | { type: 'calibration'; ok: true; point: Vec2; reason: null }
  | { type: 'calibration'; ok: false; point: null; reason: CalibrationFailure };

export type ServerMessage = WelcomeMessage | PongMessage | ErrorMessage | TrackingMessage | CalibrationMessage;
