import { useCallback, useState } from 'react';
import { clampScale, loadSettings, saveSettings, type Settings } from '../lib/settings';

/** Layout settings, persisted to localStorage (best effort). */
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
  return [settings, update];
}
