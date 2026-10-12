import type { InputEngine } from '../../input/engine';
import { useButtonPointer, usePressed } from '../../input/usePointerControls';
import type { WiimoteButton } from '../../protocol/types';

interface Props {
  engine: InputEngine<WiimoteButton>;
  side: 'left' | 'right';
}

/**
 * The B trigger. On a real remote it sits underneath, under the index finger; on a phone it is a
 * tall strip along the edge that the gripping fingers can hold while the thumb works A / D-pad.
 */
export function BZone({ engine, side }: Props) {
  const handlers = useButtonPointer(engine, 'B');
  const pressed = usePressed(engine, 'B');
  return (
    <div
      role="button"
      aria-label="B trigger"
      aria-pressed={pressed}
      data-testid="btn-B"
      data-pressed={pressed ? 'true' : 'false'}
      data-side={side}
      className="wm-bzone"
      {...handlers}
    >
      <span className="wm-bzone-grip" aria-hidden="true">
        <span className="wm-bzone-label">B</span>
      </span>
    </div>
  );
}
