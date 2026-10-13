import { useEffect, useState, useSyncExternalStore, type CSSProperties } from 'react';
import { useSettings } from '../../hooks/useSettings';
import { useWakeLock } from '../../hooks/useWakeLock';
import { ControllerSession, type SessionConfig } from '../../session/ControllerSession';
import { cameraPointingSupported } from '../../tracking/camera';
import { MotionButton, motionNotice } from '../MotionControls';
import { BZone } from './BZone';
import { CameraPointerButton, CameraPointerStatus } from './CameraPointerControls';
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
  const camera = useSyncExternalStore(session.camera.subscribe, session.camera.getState);
  const [cameraSupported] = useState(cameraPointingSupported);
  const [explainCamera, setExplainCamera] = useState(false);
  const motion = useSyncExternalStore(session.motion.subscribe, session.motion.getStatus);

  useEffect(() => {
    session.start();
    return () => {
      session.dispose();
    };
  }, [session]);

  useWakeLock(true);

  // A protocol 1 PC can't take camera frames: if the camera was switched on while connecting, turn it off
  // (its toggle is hidden for such PCs).
  useEffect(() => {
    if (state.status === 'connected' && state.welcome.protocol < 2) session.camera.disable();
  }, [state, session]);

  const link: LinkStatus =
    state.status === 'connected' ? 'connected' : state.status === 'connecting' ? 'connecting' : 'down';
  const player = state.status === 'connected' ? state.welcome.player : null;
  const side = settings.leftHanded ? 'left' : 'right';
  // Protocol 1 servers can't take camera frames; hide the toggle there.
  const serverTracks = state.status !== 'connected' || state.welcome.protocol >= 2;

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
        actions={
          <>
            <MotionButton
              status={motion}
              onToggle={() => {
                if (motion === 'on') session.motion.disable();
                else void session.motion.enable();
              }}
            />
            {serverTracks && (
              <CameraPointerButton
                state={camera}
                supported={cameraSupported}
                onToggle={() => {
                  if (!cameraSupported) setExplainCamera((v) => !v);
                  else if (camera.status === 'on' || camera.status === 'starting') session.camera.disable();
                  else void session.camera.enable();
                }}
              />
            )}
          </>
        }
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
        {state.status === 'connected' && serverTracks && (
          <CameraPointerStatus
            state={camera}
            supported={cameraSupported}
            explainUnsupported={explainCamera}
            onDismissError={() => {
              session.camera.disable();
            }}
            onCalibrate={() => {
              session.camera.calibrate();
            }}
            onResetCalibration={() => {
              session.camera.resetCalibration();
            }}
          />
        )}
        {motionNotice(motion) && (
          <div className="notice notice--warn" data-testid="motion-notice" role="status">
            <p>{motionNotice(motion)}</p>
          </div>
        )}
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
