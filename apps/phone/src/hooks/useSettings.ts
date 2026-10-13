import { useCallback, useEffect, useState } from 'react';
import { setHapticStrength } from '../lib/haptics';
import { clampScale, loadSettings, saveSettings, type Settings } from '../lib/settings';

/** Controller settings, persisted to localStorage (best effort). */
export function useSettings(): [Settings, (patch: Partial<Settings>) => void] {
  const [settings, setSettings] = useState<Settings>(() => loadSettings());
  const update = useCallback((patch: Partial<Settings>) => {
    setSettings((prev) => {
      const next: Settings = { ...prev, ...patch };
      next.scale = clampScale(next.scale);
      saveSettings(next);
      return next;
    });
  }, []);
  useEffect(() => {
    setHapticStrength(settings.haptics);
  }, [settings.haptics]);
  return [settings, update];
}
