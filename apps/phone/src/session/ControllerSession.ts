// Glue between the input engine, the WebSocket connection and the page lifecycle.

import { InputEngine } from '../input/engine';
import { Connection, type ConnectionOptions } from '../net/connection';
import { WIIMOTE_BUTTONS, type WiimoteButton } from '../protocol/types';

export interface SessionConfig {
  url: string;
  token: string;
  deviceId: string;
  name?: string;
  /** For tests. */
  createSocket?: ConnectionOptions['createSocket'];
}

export class ControllerSession {
  readonly engine = new InputEngine<WiimoteButton>(WIIMOTE_BUTTONS);
  readonly connection: Connection;
  private unbindPage: (() => void) | null = null;

  constructor(config: SessionConfig) {
    const options: ConnectionOptions = {
      url: config.url,
      hello: {
        token: config.token,
        role: 'wiimote',
        deviceId: config.deviceId,
        ...(config.name ? { name: config.name } : {}),
      },
      ...(config.createSocket ? { createSocket: config.createSocket } : {}),
    };
    this.connection = new Connection(options, {
      onReady: () => {
        this.engine.attach((msg) => this.connection.send(msg));
      },
      onDown: () => {
        // Socket already gone: stop sending, then drop everything that was held.
        this.engine.detach();
        this.engine.releaseAll();
      },
    });
  }

  start(): void {
    this.unbindPage ??= bindPageLifecycle(this);
    this.connection.start();
  }

  /** Release everything (sending one final empty snapshot if possible) and close. */
  dispose(): void {
    this.unbindPage?.();
    this.unbindPage = null;
    this.engine.releaseAllAndFlush();
    this.engine.detach();
    this.connection.stop();
  }

  /** Page hidden / window blurred: never leave a button held while the user can't see it. */
  releaseForBackground(): void {
    this.engine.releaseAllAndFlush();
  }
}

function bindPageLifecycle(session: ControllerSession): () => void {
  if (typeof window === 'undefined') return () => undefined;
  const onVisibility = () => {
    if (document.visibilityState === 'hidden') session.releaseForBackground();
    else session.connection.retryNow();
  };
  const onBackground = () => {
    session.releaseForBackground();
  };
  const onOnline = () => {
    session.connection.retryNow();
  };
  document.addEventListener('visibilitychange', onVisibility);
  window.addEventListener('blur', onBackground);
  window.addEventListener('pagehide', onBackground);
  window.addEventListener('online', onOnline);
  return () => {
    document.removeEventListener('visibilitychange', onVisibility);
    window.removeEventListener('blur', onBackground);
    window.removeEventListener('pagehide', onBackground);
    window.removeEventListener('online', onOnline);
  };
}
