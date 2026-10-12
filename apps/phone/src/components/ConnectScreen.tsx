import { useId, useState, type FormEvent } from 'react';
import type { Role } from '../protocol/types';

interface Props {
  /** Token to prefill: from the QR code URL, or the last one used. */
  initialToken: string;
  /** True when initialToken came from the QR code (shown as a chip instead of a text field). */
  tokenFromQr: boolean;
  serverUrl: string;
  initialRole: Role;
  onConnect: (token: string, role: Role) => void;
}

const ROLES: { value: Role; title: string; detail: string }[] = [
  { value: 'wiimote', title: 'Wii Remote', detail: 'Buttons, B trigger on the screen edge' },
  { value: 'gamepad', title: 'GamePad', detail: 'Coming soon' },
];

export function ConnectScreen({ initialToken, tokenFromQr, serverUrl, initialRole, onConnect }: Props) {
  const [editing, setEditing] = useState(!tokenFromQr);
  const [token, setToken] = useState(initialToken);
  const [role, setRole] = useState<Role>(initialRole);
  const tokenId = useId();
  const trimmed = token.trim();
  const canConnect = role === 'gamepad' || trimmed.length > 0;

  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (canConnect) onConnect(trimmed, role);
  };

  return (
    <div className="page" data-testid="connect-screen">
      <form className="card" onSubmit={submit}>
        <div className="brand">
          <svg viewBox="0 0 48 48" className="brand-mark" aria-hidden="true">
            <rect x="17" y="4" width="14" height="40" rx="7" />
            <circle cx="24" cy="20" r="3.5" className="brand-a" />
            <path d="M21 12h6M24 9v6" className="brand-dpad" />
          </svg>
          <div>
            <h1>Wii U Connect</h1>
            <p className="muted">Use this phone as a controller for Cemu</p>
          </div>
        </div>

        <div className="field">
          <label htmlFor={tokenId}>Pairing code</label>
          {editing ? (
            <input
              id={tokenId}
              name="token"
              className="input"
              autoComplete="off"
              autoCapitalize="off"
              autoCorrect="off"
              spellCheck={false}
              placeholder="Shown on the PC next to the QR code"
              value={token}
              onChange={(e) => {
                setToken(e.target.value);
              }}
            />
          ) : (
            <div className="token-chip" data-testid="token-from-url">
              <span>✓ From the QR code</span>
              <button
                type="button"
                className="link"
                onClick={() => {
                  setEditing(true);
                }}
              >
                Change
              </button>
            </div>
          )}
        </div>

        <fieldset className="field">
          <legend>Controller</legend>
          <div className="roles">
            {ROLES.map((r) => (
              <label key={r.value} className="role" data-checked={role === r.value}>
                <input
                  type="radio"
                  name="role"
                  value={r.value}
                  checked={role === r.value}
                  onChange={() => {
                    setRole(r.value);
                  }}
                />
                <span className="role-title">{r.title}</span>
                <span className="role-detail">{r.detail}</span>
              </label>
            ))}
          </div>
        </fieldset>

        <button type="submit" className="btn btn--block" disabled={!canConnect}>
          {role === 'gamepad' ? 'Continue' : 'Connect'}
        </button>
        <p className="muted small">PC: {serverUrl}</p>
      </form>
    </div>
  );
}
