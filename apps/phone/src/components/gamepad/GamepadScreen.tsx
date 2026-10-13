import { useEffect, useState, useSyncExternalStore } from 'react';
import { useWakeLock } from '../../hooks/useWakeLock';
import type { GamepadButton } from '../../protocol/types';
import { GamepadSession, type GamepadSessionConfig } from '../../session/GamepadSession';
import { MotionButton, motionNotice } from '../MotionControls';
import { ConnectionNotice } from '../wiimote/ConnectionNotice';
import { DPad } from '../wiimote/DPad';
import { RemoteButton } from '../wiimote/RemoteButton';
import { StatusBar } from '../wiimote/StatusBar';
import { Stick } from './Stick';
import { TouchScreen } from './TouchScreen';

interface Props {
  config: GamepadSessionConfig;
  onLeave: () => void;
}

type Engine = GamepadSession['engine'];

function Btn({ engine, button, label, cls }: { engine: Engine; button: GamepadButton; label: string; cls: string }) {
  return (
    <RemoteButton engine={engine} button={button} label={label} className={`gp-btn ${cls}`}>
      {label}
    </RemoteButton>
  );
}

export function GamepadScreen({ config, onLeave }: Props) {
  const [session] = useState(() => new GamepadSession(config));
  const state = useSyncExternalStore(session.connection.subscribe, session.connection.getState);
  const motion = useSyncExternalStore(session.motion.subscribe, session.motion.getStatus);
  const [menuOpen, setMenuOpen] = useState(false);
  const engine = session.engine;

  useEffect(() => {
    session.start();
    return () => {
      session.dispose();
    };
  }, [session]);

  useWakeLock(true);

  const notice = motionNotice(motion);
  return (
    <div
      className="gp-screen"
      data-testid="gamepad-screen"
      onContextMenu={(e) => {
        e.preventDefault();
      }}
    >
      <div className="gp-side gp-side--left">
        <div className="gp-shoulders">
          <Btn engine={engine} button="ZL" label="ZL" cls="gp-shoulder gp-shoulder--z" />
          <Btn engine={engine} button="L" label="L" cls="gp-shoulder" />
        </div>
        <Stick engine={engine} which="stick" label="Left stick" />
        <DPad engine={engine} className="gp-dpad" />
      </div>

      <div className="gp-centre">
        <StatusBar
          state={state}
          onOpenSettings={() => {
            session.releaseForBackground();
            setMenuOpen(true);
          }}
          actions={
            <MotionButton
              status={motion}
              onToggle={() => {
                if (motion === 'on') session.motion.disable();
                else void session.motion.enable();
              }}
            />
          }
        />
        <TouchScreen engine={engine}>
          <div className="gp-screen-hint">
            <strong>GamePad touchscreen</strong>
            <span>Touches here go to the game. Watch the picture in Cemu&apos;s GamePad window for now.</span>
          </div>
        </TouchScreen>
        <div className="gp-bottom">
          <Btn engine={engine} button="L3" label="L3" cls="gp-small" />
          <Btn engine={engine} button="MINUS" label="−" cls="gp-small" />
          <Btn engine={engine} button="HOME" label="⌂" cls="gp-small gp-home" />
          <Btn engine={engine} button="PLUS" label="+" cls="gp-small" />
          <Btn engine={engine} button="R3" label="R3" cls="gp-small" />
        </div>
      </div>

      <div className="gp-side gp-side--right">
        <div className="gp-shoulders">
          <Btn engine={engine} button="R" label="R" cls="gp-shoulder" />
          <Btn engine={engine} button="ZR" label="ZR" cls="gp-shoulder gp-shoulder--z" />
        </div>
        <Stick engine={engine} which="rstick" label="Right stick" />
        <div className="gp-face" role="group" aria-label="Face buttons">
          <Btn engine={engine} button="X" label="X" cls="gp-face-btn gp-face-btn--x" />
          <Btn engine={engine} button="Y" label="Y" cls="gp-face-btn gp-face-btn--y" />
          <Btn engine={engine} button="A" label="A" cls="gp-face-btn gp-face-btn--a" />
          <Btn engine={engine} button="B" label="B" cls="gp-face-btn gp-face-btn--b" />
        </div>
      </div>

      <div className="notice-slot gp-notices">
        <ConnectionNotice
          state={state}
          onRetry={() => {
            if (state.status === 'failed') session.connection.start();
            else session.connection.retryNow();
          }}
          onLeave={onLeave}
        />
        {notice && (
          <div className="notice notice--warn" data-testid="motion-notice" role="status">
            <p>{notice}</p>
          </div>
        )}
      </div>

      <div className="gp-rotate" data-testid="rotate-hint">
        <svg viewBox="0 0 48 48" aria-hidden="true">
          <rect x="16" y="6" width="16" height="28" rx="3" />
          <path d="M8 38h28a4 4 0 0 0 4-4V24" />
          <path d="m36 28 4-4 4 4" />
        </svg>
        <p>Turn your phone sideways to use it as the GamePad.</p>
      </div>

      {menuOpen && (
        <div
          className="sheet-backdrop"
          onClick={() => {
            setMenuOpen(false);
          }}
        >
          <div
            className="sheet"
            role="dialog"
            aria-label="GamePad menu"
            onClick={(e) => {
              e.stopPropagation();
            }}
          >
            <div className="sheet-head">
              <h2>GamePad</h2>
              <button
                type="button"
                className="btn btn--small btn--ghost"
                onClick={() => {
                  setMenuOpen(false);
                }}
              >
                Done
              </button>
            </div>
            <p className="muted small">
              Motion: {motion === 'on' ? 'on' : 'off'}. Tilt and turn the phone to aim in games that use the GamePad
              gyro.
            </p>
            <button type="button" className="btn btn--danger btn--block" onClick={onLeave}>
              Disconnect
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
