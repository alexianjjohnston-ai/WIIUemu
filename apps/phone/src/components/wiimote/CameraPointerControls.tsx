import type { CameraPointerState } from '../../tracking/CameraPointer';

interface ButtonProps {
  state: CameraPointerState;
  /** False on plain-http pages, where browsers block the camera. */
  supported: boolean;
  onToggle: () => void;
}

/** Top-bar toggle for camera pointing. */
export function CameraPointerButton({ state, supported, onToggle }: ButtonProps) {
  const on = state.status === 'on' || state.status === 'starting';
  const tracking = state.status === 'on' ? (state.feedback?.state ?? 'waiting') : state.status;
  return (
    <button
      type="button"
      className="icon-button camera-button"
      aria-label="Camera pointer"
      aria-pressed={on}
      data-testid="camera-toggle"
      data-state={supported ? tracking : 'unsupported'}
      onClick={onToggle}
    >
      <svg viewBox="0 0 24 24" aria-hidden="true">
        <path d="M4 8h3l2-2.5h6L17 8h3v11H4z" />
        <circle cx="12" cy="13" r="3.5" />
      </svg>
      <span className="camera-dot" aria-hidden="true" />
    </button>
  );
}

function feedbackText(state: Extract<CameraPointerState, { status: 'on' }>): { kind: string; text: string } {
  const motion = state.motion ? '' : ' · motion sensor off';
  const f = state.feedback;
  if (!f) return { kind: 'waiting', text: `Camera on · waiting for the PC${motion}` };
  switch (f.state) {
    case 'tracking':
      return { kind: 'tracking', text: `Aiming at the TV · ${f.tags} tags · ${Math.round(f.fixes_per_s)} fps${motion}` };
    case 'gyro_only':
      return { kind: 'gyro_only', text: `Tags out of view · using motion${motion}` };
    case 'lost':
      return { kind: 'lost', text: 'Point the back camera at the TV border' };
  }
}

interface StatusProps {
  state: CameraPointerState;
  supported: boolean;
  /** The user tapped the toggle on a page where the camera can't work: explain why. */
  explainUnsupported: boolean;
  onDismissError: () => void;
}

/** One line under the top bar while camera pointing is on or failed. */
export function CameraPointerStatus({ state, supported, explainUnsupported, onDismissError }: StatusProps) {
  if (!supported) {
    if (!explainUnsupported) return null;
    return (
      <div className="notice notice--warn" data-testid="camera-status" data-kind="unsupported" role="status">
        <strong>Camera pointing needs the secure link</strong>
        <p>Open the https:// address from the PC’s QR code. Browsers only allow the camera on secure pages.</p>
      </div>
    );
  }
  switch (state.status) {
    case 'off':
      return null;
    case 'starting':
      return (
        <div className="camera-line" data-testid="camera-status" data-kind="starting" role="status">
          Starting the camera…
        </div>
      );
    case 'error':
      return (
        <div className="notice notice--error" data-testid="camera-status" data-kind="error" role="alert">
          <strong>Camera pointing is off</strong>
          <p>{state.message}</p>
          <div className="notice-actions">
            <button type="button" className="btn btn--small" onClick={onDismissError}>
              OK
            </button>
          </div>
        </div>
      );
    case 'on': {
      const { kind, text } = feedbackText(state);
      return (
        <div className="camera-line" data-testid="camera-status" data-kind={kind} role="status">
          <span className="camera-dot" aria-hidden="true" />
          {text}
        </div>
      );
    }
  }
}
