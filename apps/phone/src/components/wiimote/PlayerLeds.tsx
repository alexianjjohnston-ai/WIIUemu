interface Props {
  /** 1-based player number to light, or null while not connected (LEDs blink like a searching remote). */
  player: number | null;
}

const LEDS = [1, 2, 3, 4] as const;

export function PlayerLeds({ player }: Props) {
  const searching = player === null;
  return (
    <div
      className={`wm-leds${searching ? ' wm-leds--searching' : ''}`}
      role="img"
      aria-label={searching ? 'Not connected' : `Player ${player}`}
      data-testid="player-leds"
    >
      {LEDS.map((n) => (
        <span
          key={n}
          className="wm-led"
          data-testid={`led-${n}`}
          data-lit={!searching && n === player ? 'true' : 'false'}
        />
      ))}
    </div>
  );
}
