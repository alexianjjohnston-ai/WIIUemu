import type { MotionStatus } from '../motion/MotionSensor';

/** Top-bar toggle for the phone's motion sensors. */
export function MotionButton({ status, onToggle }: { status: MotionStatus; onToggle: () => void }) {
  return (
    <button
      type="button"
      className="icon-button motion-button"
      aria-label="Motion controls"
      aria-pressed={status === 'on'}
      data-testid="motion-toggle"
      data-state={status}
      onClick={onToggle}
    >
      <svg viewBox="0 0 24 24" aria-hidden="true">
        <rect x="7" y="3" width="10" height="18" rx="2" />
        <path d="M3 9a6 6 0 0 0 0 6M21 9a6 6 0 0 1 0 6" />
      </svg>
    </button>
  );
}

export function motionNotice(status: MotionStatus): string | null {
  switch (status) {
    case 'denied':
      return 'Motion access was blocked. Allow “Motion & Orientation Access” for this site, then tap the motion button again.';
    case 'unsupported':
      return 'This browser has no motion sensors. Buttons, sticks and the touchscreen still work.';
    default:
      return null;
  }
}
