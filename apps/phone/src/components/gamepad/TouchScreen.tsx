import { useMemo, useState, type PointerEvent as ReactPointerEvent, type ReactNode } from 'react';
import { touchPosition } from '../../input/analog';
import type { InputEngine } from '../../input/engine';
import type { GamepadButton, Vec2 } from '../../protocol/types';

interface Props {
  engine: InputEngine<GamepadButton>;
  children?: ReactNode;
}

/** The GamePad touchscreen: the first finger on it is sent as the touch position; lifting it ends the touch. */
export function TouchScreen({ engine, children }: Props) {
  const [dot, setDot] = useState<Vec2 | null>(null);
  const handlers = useMemo(() => {
    let active: number | null = null;
    const update = (e: ReactPointerEvent<HTMLElement>) => {
      const p = touchPosition(e.clientX, e.clientY, e.currentTarget.getBoundingClientRect());
      engine.setPointer(p);
      setDot(p);
    };
    const end = (e: ReactPointerEvent<HTMLElement>) => {
      if (e.pointerId !== active) return;
      active = null;
      engine.setPointer(null);
      setDot(null);
    };
    return {
      onPointerDown: (e: ReactPointerEvent<HTMLElement>) => {
        if (active !== null || (e.pointerType === 'mouse' && e.button !== 0)) return;
        e.preventDefault();
        active = e.pointerId;
        try {
          e.currentTarget.setPointerCapture(e.pointerId);
        } catch {
          // Synthetic or already-gone pointer.
        }
        update(e);
      },
      onPointerMove: (e: ReactPointerEvent<HTMLElement>) => {
        if (e.pointerId === active) update(e);
      },
      onPointerUp: end,
      onPointerCancel: end,
      onLostPointerCapture: end,
    };
  }, [engine]);

  return (
    <div className="gp-touch-wrap">
      <div
        className="gp-touch"
        role="application"
        aria-label="GamePad touchscreen"
        data-testid="touchscreen"
        data-touching={dot ? 'true' : 'false'}
        {...handlers}
      >
        {children}
        {dot && (
          <span
            className="gp-touch-dot"
            style={{ left: `${dot.x * 100}%`, top: `${dot.y * 100}%` }}
            aria-hidden="true"
          />
        )}
      </div>
    </div>
  );
}
