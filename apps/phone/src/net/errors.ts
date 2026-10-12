import type { ErrorCode } from '../protocol/types';

/** Client-side failure reasons in addition to the protocol's error codes. */
export type LocalErrorCode = 'network' | 'handshake_timeout' | 'stale';

export interface ConnError {
  code: ErrorCode | LocalErrorCode | (string & {});
  /** Raw message from the server, if any (for diagnostics). */
  detail?: string;
}

interface Description {
  title: string;
  hint: string;
  /** Terminal errors stop automatic reconnects; the user must act. */
  terminal: boolean;
}

const DESCRIPTIONS: Record<ErrorCode | LocalErrorCode, Description> = {
  bad_token: {
    title: 'Pairing code rejected',
    hint: 'Scan the QR code shown on the PC again, or re-enter the pairing code.',
    terminal: true,
  },
  unsupported_protocol: {
    title: 'App version mismatch',
    hint: 'This phone page and the PC app speak different protocol versions. Reload the page from the PC’s QR code.',
    terminal: true,
  },
  invalid_message: {
    title: 'The PC rejected a message',
    hint: 'This is a bug in the phone app. Tap “Try again”; if it keeps happening, reload the page.',
    terminal: true,
  },
  role_unavailable: {
    title: 'No free controller slot',
    hint: 'All Wii Remote slots are taken (up to 3). Disconnect another phone on the PC, then try again.',
    terminal: true,
  },
  timeout: {
    title: 'The PC did not hear from this phone in time',
    hint: 'Reconnecting automatically…',
    terminal: false,
  },
  replaced: {
    title: 'Opened somewhere else',
    hint: 'This controller connected again from another tab or window, so this one was disconnected.',
    terminal: true,
  },
  network: {
    title: 'Connection to the PC lost',
    hint: 'Check that the phone is on the same Wi-Fi as the PC and that Wii U Connect is running. Reconnecting automatically…',
    terminal: false,
  },
  handshake_timeout: {
    title: 'The PC did not answer',
    hint: 'Reconnecting automatically…',
    terminal: false,
  },
  stale: {
    title: 'Connection stalled',
    hint: 'No reply from the PC for a few seconds. Reconnecting automatically…',
    terminal: false,
  },
};

export function describeError(error: ConnError): Description {
  const known = (DESCRIPTIONS as Record<string, Description | undefined>)[error.code];
  if (known) return known;
  return {
    title: 'Unexpected error from the PC',
    hint: error.detail ? `“${error.detail}”. Reconnecting automatically…` : 'Reconnecting automatically…',
    terminal: false,
  };
}

export function isTerminal(error: ConnError): boolean {
  return describeError(error).terminal;
}
