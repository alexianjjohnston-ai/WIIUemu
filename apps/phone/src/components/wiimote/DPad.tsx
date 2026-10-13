import type { InputEngine } from '../../input/engine';
import { useDpadPointer, usePressed } from '../../input/usePointerControls';
import type { DpadDirection } from '../../input/dpad';

interface Props<B extends string> {
  engine: InputEngine<B | DpadDirection>;
  className?: string;
}

const ARMS = [
  ['UP', 'up'],
  ['DOWN', 'down'],
  ['LEFT', 'left'],
  ['RIGHT', 'right'],
] as const;

function Arm<B extends string>({ engine, dir, cls }: { engine: Props<B>['engine']; dir: DpadDirection; cls: string }) {
  const pressed = usePressed(engine, dir);
  return (
    <span
      className={`wm-dpad-arm wm-dpad-arm--${cls}`}
      data-testid={`btn-${dir}`}
      data-pressed={pressed ? 'true' : 'false'}
    />
  );
}

/** Cross-shaped D-pad. One element receives the touch; the direction follows the thumb. */
export function DPad<B extends string>({ engine, className = '' }: Props<B>) {
  const handlers = useDpadPointer(engine);
  return (
    <div className={`wm-hit wm-dpad ${className}`} role="group" aria-label="D-pad" data-testid="dpad" {...handlers}>
      <span className="wm-dpad-cross">
        {ARMS.map(([dir, cls]) => (
          <Arm key={dir} engine={engine} dir={dir} cls={cls} />
        ))}
        <span className="wm-dpad-center" />
      </span>
    </div>
  );
}
