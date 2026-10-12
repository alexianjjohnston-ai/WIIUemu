import type { ReactNode } from 'react';
import type { InputEngine } from '../../input/engine';
import { useButtonPointer, usePressed } from '../../input/usePointerControls';
import type { WiimoteButton } from '../../protocol/types';

interface Props {
  engine: InputEngine<WiimoteButton>;
  button: WiimoteButton;
  label: string;
  className: string;
  children?: ReactNode;
}

/** A pressable control. The element is the hit area; its children draw the visible cap. */
export function RemoteButton({ engine, button, label, className, children }: Props) {
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
