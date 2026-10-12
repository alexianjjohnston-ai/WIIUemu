/** Short buzz on button press. iOS Safari has no Vibration API, so this is a guarded no-op there. */
export function hapticTap(ms = 10): void {
  try {
    if (typeof navigator !== 'undefined' && typeof navigator.vibrate === 'function') {
      navigator.vibrate(ms);
    }
  } catch {
    // Some browsers throw when vibration is blocked (e.g. before a user gesture).
  }
}
