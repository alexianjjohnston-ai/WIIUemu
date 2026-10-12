import { useMemo, useState, type PointerEvent as ReactPointerEvent } from 'react';
import { stickValue } from '../../input/analog';
import type { InputEngine, StickName } from '../../input/engine';
import type { GamepadButton, Vec2 } from '../../protocol/types';

interface Props {
  engine: InputEngine<GamepadButton>;
  which: StickName;
  label: string;
}

/** Fraction of the base radius the knob can travel. */
const TRAVEL = 0.62;

/** Analog stick: one finger at a time; the value follows the thumb and springs back on release. */
export function Stick({ engine, which, label }: Props) {
  const [knob, setKnob] = useState<Vec2>({ x: 0, y: 0 });
  const handlers = useMemo(() => {
    let active: number | null = null;
    const update = (e: ReactPointerEvent<HTMLElement>) => {
      const rect = e.currentTarget.getBoundingClientRect();
      const radius = (Math.min(rect.width, rect.height) / 2) * TRAVEL;
      const v = stickValue(e.clientX - (rect.left + rect.width / 2), e.clientY - (rect.top + rect.height / 2), radius);
      engine.setStick(which, v);
      setKnob(v);
    };
    const end = (e: ReactPointerEvent<HTMLElement>) => {
      if (e.pointerId !== active) return;
      active = null;
      engine.setStick(which, { x: 0, y: 0 });
      setKnob({ x: 0, y: 0 });
    };
    return {
      onPointerDown: (e: ReactPointerEvent<HTMLElement>) => {
        if (active !== null || (e.pointerType === 'mouse' && e.button !== 0)) return;
        e.preventDefault();
        active = e.pointerId;
        try {
          e.currentTarget.setPointerCapture(e.pointerId);
        } catch {
          // Synthetic or already-gone pointer: up/cancel still end it.
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
  }, [engine, which]);

  const held = knob.x !== 0 || knob.y !== 0;
  return (
    <div
      className="gp-stick"
      role="group"
      aria-label={label}
      data-testid={which}
      data-x={knob.x}
      data-y={knob.y}
      data-held={held ? 'true' : 'false'}
      {...handlers}
    >
      <span
        className="gp-knob"
        style={{
          transform: `translate(${knob.x * TRAVEL * 50}%, ${-knob.y * TRAVEL * 50}%)`,
        }}
        aria-hidden="true"
      />
    </div>
  );
}
