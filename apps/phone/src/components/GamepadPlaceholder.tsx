interface Props {
  onBack: () => void;
}

/** The GamePad role arrives in a later milestone (screen streaming, sticks, touch). */
export function GamepadPlaceholder({ onBack }: Props) {
  return (
    <div className="page" data-testid="gamepad-placeholder">
      <div className="card">
        <svg className="placeholder-art" viewBox="0 0 120 72" aria-hidden="true">
          <rect x="4" y="8" width="112" height="56" rx="24" />
          <rect x="34" y="18" width="52" height="36" rx="4" className="screen" />
          <circle cx="18" cy="28" r="6" />
          <circle cx="102" cy="44" r="6" />
        </svg>
        <h1>GamePad: coming soon</h1>
        <p>
          The GamePad role (with the game screen streamed to the phone) is not part of this version yet. For now,
          use this phone as a Wii Remote.
        </p>
        <button type="button" className="btn btn--block" onClick={onBack}>
          Back
        </button>
      </div>
    </div>
  );
}
