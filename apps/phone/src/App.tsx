import { useMemo, useState } from 'react';
import { ConnectScreen } from './components/ConnectScreen';
import { GamepadPlaceholder } from './components/GamepadPlaceholder';
import { WiimoteScreen } from './components/wiimote/WiimoteScreen';
import { getDeviceId } from './lib/deviceId';
import { guessDeviceName, readLaunchParams, resolveWsUrl } from './lib/params';
import { loadLastToken, saveLastToken } from './lib/tokenStore';
import type { SessionConfig } from './session/ControllerSession';

type Screen = { name: 'connect' } | { name: 'wiimote'; config: SessionConfig; key: number } | { name: 'gamepad' };

export function App() {
  const launch = useMemo(() => {
    const params = readLaunchParams(window.location.search);
    return {
      ...params,
      url: resolveWsUrl(window.location, params.server),
    };
  }, []);
  // Token from the QR code wins; otherwise the last one used (installed PWAs start without ?token=).
  const [token, setToken] = useState(() => launch.token ?? loadLastToken() ?? '');
  const [screen, setScreen] = useState<Screen>({ name: 'connect' });
  const [lastRole, setLastRole] = useState<'wiimote' | 'gamepad'>('wiimote');
  const back = () => {
    setScreen({ name: 'connect' });
  };

  switch (screen.name) {
    case 'wiimote':
      return <WiimoteScreen key={screen.key} config={screen.config} onLeave={back} />;
    case 'gamepad':
      return <GamepadPlaceholder onBack={back} />;
    case 'connect':
      return (
        <ConnectScreen
          initialToken={token}
          tokenFromQr={launch.token !== null && token === launch.token}
          serverUrl={launch.url}
          initialRole={lastRole}
          onConnect={(nextToken, role) => {
            setLastRole(role);
            setToken(nextToken);
            if (role === 'gamepad') {
              setScreen({ name: 'gamepad' });
              return;
            }
            saveLastToken(nextToken);
            setScreen({
              name: 'wiimote',
              key: Date.now(),
              config: { url: launch.url, token: nextToken, deviceId: getDeviceId(), name: guessDeviceName(navigator.userAgent) },
            });
          }}
        />
      );
  }
}
