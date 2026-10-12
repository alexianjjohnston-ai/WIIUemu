import { useEffect } from 'react';

/**
 * Keep the screen on while `active`. The Screen Wake Lock API needs a secure context and is
 * missing on older browsers; every step is guarded. The lock is dropped by the browser whenever the
 * page is hidden, so it is requested again when the page becomes visible.
 */
export function useWakeLock(active: boolean): void {
  useEffect(() => {
    if (!active || typeof navigator === 'undefined' || !('wakeLock' in navigator)) return;
    let sentinel: WakeLockSentinel | null = null;
    let cancelled = false;

    const request = async () => {
      if (document.visibilityState !== 'visible' || sentinel) return;
      try {
        const lock = await navigator.wakeLock.request('screen');
        if (cancelled) {
          void lock.release().catch(() => undefined);
          return;
        }
        sentinel = lock;
        lock.addEventListener('release', () => {
          if (sentinel === lock) sentinel = null;
        });
      } catch {
        // Denied, insecure context, or low battery mode: the controller still works.
      }
    };

    const onVisibility = () => {
      void request();
    };
    void request();
    document.addEventListener('visibilitychange', onVisibility);
    return () => {
      cancelled = true;
      document.removeEventListener('visibilitychange', onVisibility);
      void sentinel?.release().catch(() => undefined);
      sentinel = null;
    };
  }, [active]);
}
