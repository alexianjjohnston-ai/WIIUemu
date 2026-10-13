// Page lifecycle shared by every controller session: never leave input held while the user can't see the page.

import type { Connection } from '../net/connection';

export interface LifecycleTarget {
  connection: Pick<Connection, 'retryNow'>;
  /** Release every button, stick and touch (one final empty snapshot). */
  releaseForBackground: () => void;
  /** Page hidden: stop sensors and cameras. */
  onHidden?: () => void;
  /** Page visible again. */
  onVisible?: () => void;
}

export function bindPageLifecycle(session: LifecycleTarget): () => void {
  if (typeof window === 'undefined') return () => undefined;
  const onVisibility = () => {
    if (document.visibilityState === 'hidden') {
      session.releaseForBackground();
      session.onHidden?.();
    } else {
      session.connection.retryNow();
      session.onVisible?.();
    }
  };
  const onBackground = () => {
    session.releaseForBackground();
  };
  const onOnline = () => {
    session.connection.retryNow();
  };
  document.addEventListener('visibilitychange', onVisibility);
  window.addEventListener('blur', onBackground);
  window.addEventListener('pagehide', onBackground);
  window.addEventListener('online', onOnline);
  return () => {
    document.removeEventListener('visibilitychange', onVisibility);
    window.removeEventListener('blur', onBackground);
    window.removeEventListener('pagehide', onBackground);
    window.removeEventListener('online', onOnline);
  };
}
