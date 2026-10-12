// GamePad controller session: buttons, both sticks, the touchscreen and motion over one WebSocket connection.

import { InputEngine } from '../input/engine';
import { MotionSensor, type MotionSensorDeps } from '../motion/MotionSensor';
import { Connection, type ConnectionOptions } from '../net/connection';
import { GAMEPAD_BUTTONS, type GamepadButton } from '../protocol/types';
import type { SessionConfig } from './ControllerSession';
import { bindPageLifecycle } from './lifecycle';

export interface GamepadSessionConfig extends SessionConfig {
  /** For tests. */
  motion?: Omit<MotionSensorDeps, 'onSample'>;
}

export class GamepadSession {
  readonly engine = new InputEngine<GamepadButton>(GAMEPAD_BUTTONS, {
    sticks: true,
  });
  readonly connection: Connection;
  /** Off until the user turns it on (iOS needs a tap to grant motion access). */
  readonly motion: MotionSensor;
  private unbindPage: (() => void) | null = null;
  /** Motion was on when the page was hidden. */
  private resumeMotion = false;

  constructor(config: GamepadSessionConfig) {
    const options: ConnectionOptions = {
      url: config.url,
      hello: {
        token: config.token,
        role: 'gamepad',
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
        this.engine.detach();
        this.engine.releaseAll();
      },
    });
    this.motion = new MotionSensor('face', {
      ...config.motion,
      onSample: () => {
        this.engine.motionChanged();
      },
    });
    this.engine.setMotionSource(this.motion.getLatest);
  }

  start(): void {
    this.unbindPage ??= bindPageLifecycle({
      connection: this.connection,
      releaseForBackground: () => {
        this.releaseForBackground();
      },
      onHidden: () => {
        this.resumeMotion = this.motion.getStatus() === 'on';
        this.motion.disable();
      },
      onVisible: () => {
        // Permission was already granted inside the original tap, so no gesture is needed to resume.
        if (this.resumeMotion) void this.motion.enable();
        this.resumeMotion = false;
      },
    });
    this.connection.start();
  }

  dispose(): void {
    this.unbindPage?.();
    this.unbindPage = null;
    this.motion.disable();
    this.engine.releaseAllAndFlush();
    this.engine.detach();
    this.connection.stop();
  }

  releaseForBackground(): void {
    this.engine.releaseAllAndFlush();
  }
}
