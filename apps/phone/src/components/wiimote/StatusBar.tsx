import type { ReactNode } from 'react';
import type { ConnectionState } from '../../net/connection';

interface Props {
  state: ConnectionState;
  onOpenSettings: () => void;
  /** Extra buttons left of the settings button. */
  actions?: ReactNode;
}

function label(state: ConnectionState): string {
  switch (state.status) {
    case 'connected': {
      const rtt = state.rttMs === null ? '' : ` · ${state.rttMs} ms`;
      return `Player ${state.welcome.player} · Connected${rtt}`;
    }
    case 'connecting':
      return 'Connecting…';
    case 'reconnecting':
      return 'Disconnected · reconnecting…';
    case 'failed':
      return 'Disconnected';
    case 'idle':
      return 'Not connected';
  }
}

export function StatusBar({ state, onOpenSettings, actions }: Props) {
  return (
    <header className="topbar">
      <span className="status-pill" data-status={state.status} data-testid="status" role="status">
        <span className="status-dot" aria-hidden="true" />
        {label(state)}
      </span>
      <div className="topbar-actions">
        {actions}
        <button type="button" className="icon-button" aria-label="Settings" onClick={onOpenSettings}>
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="M4 7h10M18 7h2M4 17h4M12 17h8" />
          <circle cx="16" cy="7" r="2" />
          <circle cx="10" cy="17" r="2" />
          </svg>
        </button>
      </div>
    </header>
  );
}
