import type { HapticStrength } from '../lib/haptics';

const OPTIONS: { value: HapticStrength; label: string }[] = [
  { value: 'off', label: 'Off' },
  { value: 'light', label: 'Light' },
  { value: 'strong', label: 'Strong' },
];

interface Props {
  value: HapticStrength;
  onChange: (value: HapticStrength) => void;
}

/** Vibration strength for presses, the sticks' rim, connecting and calibration. Shared by both controllers. */
export function HapticsSetting({ value, onChange }: Props) {
  return (
    <fieldset className="field" data-testid="haptics-setting">
      <legend>
        Vibration
        <span className="field-hint">On iPhone it needs iOS 18 or newer and has one strength.</span>
      </legend>
      <div className="segmented segmented--compact">
        {OPTIONS.map((o) => (
          <label key={o.value} className="segment" data-checked={value === o.value}>
            <input
              type="radio"
              name="haptics"
              value={o.value}
              checked={value === o.value}
              onChange={() => {
                onChange(o.value);
              }}
            />
            <span className="segment-label">{o.label}</span>
          </label>
        ))}
      </div>
    </fieldset>
  );
}
