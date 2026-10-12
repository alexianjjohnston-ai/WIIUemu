import { useEffect, useState, useSyncExternalStore, type CSSProperties } from 'react';
import { useSettings } from '../../hooks/useSettings';
import { useWakeLock } from '../../hooks/useWakeLock';
import { ControllerSession, type SessionConfig } from '../../session/ControllerSession';
import { BZone } from './BZone';
import { ConnectionNotice } from './ConnectionNotice';
import { RemoteBody, type LinkStatus } from './RemoteBody';
import { SettingsSheet } from './SettingsSheet';
import { StatusBar } from './StatusBar';

interface Props {
  config: SessionConfig;
  onLeave: () => void;
}

export function WiimoteScreen({ config, onLeave }: Props) {
  // One session per mount; the parent re-keys this component to start a new one.
  const [session] = useState(() => new ControllerSession(config));
  const state = useSyncExternalStore(session.connection.subscribe, session.connection.getState);
  const [settings, updateSettings] = useSettings();
  const [settingsOpen, setSettingsOpen] = useState(false);

  useEffect(() => {
    session.start();
    return () => {
      session.dispose();
    };
  }, [session]);

  useWakeLock(true);

  const link: LinkStatus =
    state.status === 'connected' ? 'connected' : state.status === 'connecting' ? 'connecting' : 'down';
  const player = state.status === 'connected' ? state.welcome.player : null;
  const side = settings.leftHanded ? 'left' : 'right';

  return (
    <div
      className="wm-screen"
      data-testid="wiimote-screen"
      data-layout={settings.layout}
      data-hand={side}
      style={{ '--scale': settings.scale } as CSSProperties}
      onContextMenu={(e) => {
        e.preventDefault();
      }}
    >
      <StatusBar
        state={state}
        onOpenSettings={() => {
          session.releaseForBackground();
          setSettingsOpen(true);
        }}
      />
      <div className="notice-slot">
        <ConnectionNotice
          state={state}
          onRetry={() => {
            if (state.status === 'failed') session.connection.start();
            else session.connection.retryNow();
          }}
          onLeave={onLeave}
        />
      </div>
      <main className="wm-main">
        <div className="wm-body-col">
          <RemoteBody engine={session.engine} player={player} link={link} />
        </div>
        <BZone engine={session.engine} side={side} />
      </main>
      {settingsOpen && (
        <SettingsSheet
          settings={settings}
          onChange={updateSettings}
          onClose={() => {
            setSettingsOpen(false);
          }}
          onLeave={onLeave}
        />
      )}
    </div>
  );
}
