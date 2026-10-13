import { useEffect, useRef } from 'react';
import { haptic, type HapticEvent } from '../lib/haptics';

/** Buzzes when `value` changes in a way `toEvent` names (connection drops, calibration results, …). */
export function useHapticTransition<T>(value: T, toEvent: (prev: T, next: T) => HapticEvent | null): void {
  const prev = useRef(value);
  useEffect(() => {
    const event = toEvent(prev.current, value);
    prev.current = value;
    if (event) haptic(event);
  }, [value, toEvent]);
}
