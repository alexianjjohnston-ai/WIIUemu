import { useId } from 'react';
import { HapticsSetting } from '../HapticsSetting';
import { SCALE_MAX, SCALE_MIN, SCALE_STEP, type LayoutName, type Settings } from '../../lib/settings';

interface Props {
  settings: Settings;
  onChange: (patch: Partial<Settings>) => void;
  onClose: () => void;
  onLeave: () => void;
}

const LAYOUTS: { value: LayoutName; label: string; hint: string }[] = [
  { value: 'authentic', label: 'Authentic', hint: 'Slim remote, real proportions' },
  { value: 'comfort', label: 'Comfort', hint: 'Bigger buttons, wider B zone' },
];

export function SettingsSheet({ settings, onChange, onClose, onLeave }: Props) {
  const scaleId = useId();
  const handId = useId();
  return (
    <div className="sheet-backdrop" onClick={onClose}>
      <div
        className="sheet"
        role="dialog"
        aria-modal="true"
        aria-label="Controller settings"
        onClick={(e) => {
          e.stopPropagation();
        }}
      >
        <div className="sheet-head">
          <h2>Settings</h2>
          <button type="button" className="btn btn--small btn--ghost" onClick={onClose}>
            Done
          </button>
        </div>

        <fieldset className="field">
          <legend>Layout</legend>
          <div className="segmented">
            {LAYOUTS.map((l) => (
              <label key={l.value} className="segment" data-checked={settings.layout === l.value}>
                <input
                  type="radio"
                  name="layout"
                  value={l.value}
                  checked={settings.layout === l.value}
                  onChange={() => {
                    onChange({ layout: l.value });
                  }}
                />
                <span className="segment-label">{l.label}</span>
                <span className="segment-hint">{l.hint}</span>
              </label>
            ))}
          </div>
        </fieldset>

        <div className="field field--row">
          <label htmlFor={handId}>
            Left-handed
            <span className="field-hint">B zone on the left edge</span>
          </label>
          <input
            id={handId}
            type="checkbox"
            className="switch"
            checked={settings.leftHanded}
            onChange={(e) => {
              onChange({ leftHanded: e.target.checked });
            }}
          />
        </div>

        <div className="field">
          <label htmlFor={scaleId}>
            Button size <output>{Math.round(settings.scale * 100)}%</output>
          </label>
          <input
            id={scaleId}
            type="range"
            min={SCALE_MIN}
            max={SCALE_MAX}
            step={SCALE_STEP}
            value={settings.scale}
            onChange={(e) => {
              onChange({ scale: Number(e.target.value) });
            }}
          />
        </div>

        <HapticsSetting
          value={settings.haptics}
          onChange={(haptics) => {
            onChange({ haptics });
          }}
        />

        <button type="button" className="btn btn--danger btn--block" onClick={onLeave}>
          Disconnect
        </button>
      </div>
    </div>
  );
}
