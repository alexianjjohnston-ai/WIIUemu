import { useEffect, useState } from 'react';
import type { ConnectionState } from '../../net/connection';
import { describeError } from '../../net/errors';

interface Props {
  state: ConnectionState;
  onRetry: () => void;
  onLeave: () => void;
}

/** "retrying in 3s", counting down from mount. Re-mount (via key) for each attempt. */
function RetryCountdown({ ms }: { ms: number }) {
  const [until] = useState(() => Date.now() + ms);
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = setInterval(() => {
      setNow(Date.now());
    }, 250);
    return () => {
      clearInterval(id);
    };
  }, []);
  const left = Math.ceil((until - now) / 1000);
  return <>{left > 0 ? ` · retrying in ${left}s` : ' · retrying…'}</>;
}

/** Banner over the controller whenever the link is not up. Hidden while connected. */
export function ConnectionNotice({ state, onRetry, onLeave }: Props) {
  if (state.status === 'connected' || state.status === 'idle') return null;

  if (state.status === 'connecting') {
    return (
      <div className="notice notice--info" data-testid="connection-notice" data-kind="connecting">
        <strong>Connecting to the PC…</strong>
        {state.attempt > 1 && <span className="notice-meta"> Attempt {state.attempt}</span>}
      </div>
    );
  }

  const d = describeError(state.reason);
  if (state.status === 'reconnecting') {
    return (
      <div className="notice notice--warn" data-testid="connection-notice" data-kind="reconnecting" role="alert">
        <strong>{d.title}</strong>
        <p>{d.hint}</p>
        <div className="notice-actions">
          <span className="notice-meta">
            Attempt {state.attempt}
            <RetryCountdown key={state.attempt} ms={state.retryInMs} />
          </span>
          <button type="button" className="btn btn--small" onClick={onRetry}>
            Retry now
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="notice notice--error" data-testid="connection-notice" data-kind="failed" role="alert">
      <strong>{d.title}</strong>
      <p>{d.hint}</p>
      {state.reason.detail && <p className="notice-meta">PC said: “{state.reason.detail}”</p>}
      <div className="notice-actions">
        <button type="button" className="btn btn--small btn--ghost" onClick={onLeave}>
          Back
        </button>
        <button type="button" className="btn btn--small" onClick={onRetry}>
          Try again
        </button>
      </div>
    </div>
  );
}
