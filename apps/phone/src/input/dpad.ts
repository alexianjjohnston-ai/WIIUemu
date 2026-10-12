import type { WiimoteButton } from '../protocol/types';

export type DpadDirection = Extract<WiimoteButton, 'UP' | 'DOWN' | 'LEFT' | 'RIGHT'>;

/** Fraction of the D-pad radius around the centre where nothing is pressed (the pivot). */
export const DPAD_DEADZONE = 0.18;
/** Half-width in degrees of each diagonal sector (centred on 45°, 135°, ...). */
export const DPAD_DIAGONAL_HALF_ANGLE = 15;

/**
 * Directions for a touch at (dx, dy) relative to the D-pad centre, screen coordinates (+y down),
 * `radius` = half the D-pad size. Cardinal sectors are wider than diagonal ones so a slightly
 * off-axis thumb still registers a single direction, like rolling a real D-pad.
 */
export function dpadDirections(dx: number, dy: number, radius: number): DpadDirection[] {
  if (!(radius > 0) || !Number.isFinite(dx) || !Number.isFinite(dy)) return [];
  if (Math.hypot(dx, dy) < radius * DPAD_DEADZONE) return [];
  // 0° = right, 90° = up.
  const deg = ((Math.atan2(-dy, dx) * 180) / Math.PI + 360) % 360;
  const dirs: DpadDirection[] = [];
  const near = (center: number) => {
    const diff = Math.abs(((deg - center + 540) % 360) - 180);
    return diff <= 45 + DPAD_DIAGONAL_HALF_ANGLE;
  };
  if (near(90)) dirs.push('UP');
  if (near(270)) dirs.push('DOWN');
  if (near(180)) dirs.push('LEFT');
  if (near(0)) dirs.push('RIGHT');
  return dirs;
}
