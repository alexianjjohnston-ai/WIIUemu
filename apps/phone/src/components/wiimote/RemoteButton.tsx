import type { ReactNode } from 'react';
import type { InputEngine } from '../../input/engine';
import { useButtonPointer, usePressed } from '../../input/usePointerControls';

interface Props<B extends string> {
  engine: InputEngine<B>;
  button: B;
  label: string;
  className: string;
  children?: ReactNode;
}

/** A pressable control. The element is the hit area; its children draw the visible cap. */
export function RemoteButton<B extends string>({ engine, button, label, className, children }: Props<B>) {
  const handlers = useButtonPointer(engine, button);
  const pressed = usePressed(engine, button);
  return (
    <div
      role="button"
      aria-label={label}
      aria-pressed={pressed}
      data-testid={`btn-${button}`}
      data-pressed={pressed ? 'true' : 'false'}
      className={`wm-hit ${className}`}
      {...handlers}
    >
      <span className="wm-cap">{children}</span>
    </div>
  );
}
