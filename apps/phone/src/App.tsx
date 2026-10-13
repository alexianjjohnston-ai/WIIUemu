import { useMemo, useState } from 'react';
import { ConnectScreen } from './components/ConnectScreen';
import { GamepadScreen } from './components/gamepad/GamepadScreen';
import { WiimoteScreen } from './components/wiimote/WiimoteScreen';
import { getDeviceId } from './lib/deviceId';
import { guessDeviceName, readLaunchParams, resolveWsUrl } from './lib/params';
import { loadLastToken, saveLastToken } from './lib/tokenStore';
import type { SessionConfig } from './session/ControllerSession';

type Screen =
  | { name: 'connect' }
  | { name: 'wiimote'; config: SessionConfig; key: number }
  | { name: 'gamepad'; config: SessionConfig; key: number };

export function App() {
  const launch = useMemo(() => {
    const params = readLaunchParams(window.location.search);
    return {
      ...params,
      // `?server=` is a dev-server convenience only. Production builds are served by the PC itself, and honouring
      // the override there would let a crafted link send the pairing token to another host.
      url: resolveWsUrl(window.location, import.meta.env.DEV ? params.server : null),
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
      return <GamepadScreen key={screen.key} config={screen.config} onLeave={back} />;
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
            saveLastToken(nextToken);
            const config = {
              url: launch.url,
              token: nextToken,
              deviceId: getDeviceId(),
              name: guessDeviceName(navigator.userAgent),
            };
            setScreen(
              role === 'gamepad'
                ? { name: 'gamepad', key: Date.now(), config }
                : { name: 'wiimote', key: Date.now(), config },
            );
          }}
        />
      );
  }
}
