import type { InputEngine } from '../../input/engine';
import type { WiimoteButton } from '../../protocol/types';
import { DPad } from './DPad';
import { PlayerLeds } from './PlayerLeds';
import { RemoteButton } from './RemoteButton';

export type LinkStatus = 'connected' | 'connecting' | 'down';

interface Props {
  engine: InputEngine<WiimoteButton>;
  player: number | null;
  link: LinkStatus;
}

/** The white remote, top to bottom: status + player LEDs, D-pad, A, − HOME +, speaker, 1, 2. */
export function RemoteBody({ engine, player, link }: Props) {
  return (
    <div className="wm-remote" data-testid="remote">
      <div className="wm-remote-inner">
        <div className="wm-top">
          <span className={`wm-power wm-power--${link}`} aria-hidden="true">
            <svg viewBox="0 0 24 24" className="wm-power-icon">
              <path d="M12 3v8" />
              <path d="M7 6.3a7.5 7.5 0 1 0 10 0" />
            </svg>
          </span>
          <PlayerLeds player={player} />
        </div>

        <DPad engine={engine} />

        <RemoteButton engine={engine} button="A" label="A" className="wm-a">
          A
        </RemoteButton>

        <div className="wm-row">
          <RemoteButton engine={engine} button="MINUS" label="Minus" className="wm-small">
            <svg viewBox="0 0 24 24" className="wm-glyph" aria-hidden="true">
              <path d="M6 12h12" />
            </svg>
          </RemoteButton>
          <RemoteButton engine={engine} button="HOME" label="Home" className="wm-small wm-home">
            <svg viewBox="0 0 24 24" className="wm-glyph" aria-hidden="true">
              <path d="M4 12 12 5l8 7M7 10v9h10v-9" />
            </svg>
          </RemoteButton>
          <RemoteButton engine={engine} button="PLUS" label="Plus" className="wm-small">
            <svg viewBox="0 0 24 24" className="wm-glyph" aria-hidden="true">
              <path d="M6 12h12M12 6v12" />
            </svg>
          </RemoteButton>
        </div>

        <span className="wm-speaker" aria-hidden="true" />

        <div className="wm-numbers">
          <RemoteButton engine={engine} button="ONE" label="1" className="wm-num">
            1
          </RemoteButton>
          <RemoteButton engine={engine} button="TWO" label="2" className="wm-num">
            2
          </RemoteButton>
        </div>
      </div>
    </div>
  );
}
