// Glue between the input engine, the WebSocket connection and the page lifecycle.

import { InputEngine } from '../input/engine';
import { MotionSensor, type MotionSensorDeps } from '../motion/MotionSensor';
import { Connection, type ConnectionOptions } from '../net/connection';
import { WIIMOTE_BUTTONS, type WiimoteButton } from '../protocol/types';
import { CameraPointer, type CameraPointerDeps } from '../tracking/CameraPointer';
import { bindPageLifecycle } from './lifecycle';

export interface SessionConfig {
  url: string;
  token: string;
  deviceId: string;
  name?: string;
  /** For tests. */
  createSocket?: ConnectionOptions['createSocket'];
  createCamera?: CameraPointerDeps['createCamera'];
  motion?: Omit<MotionSensorDeps, 'onSample'>;
}

export class ControllerSession {
  readonly engine = new InputEngine<WiimoteButton>(WIIMOTE_BUTTONS);
  readonly connection: Connection;
  /** Protocol 2 camera pointing; off until the user turns it on. */
  readonly camera: CameraPointer;
  /** Wii Remote accelerometer + MotionPlus from the phone's sensors; off until the user turns it on. */
  readonly motion: MotionSensor;
  private unbindPage: (() => void) | null = null;
  private resumeMotion = false;

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
        this.camera.onConnectionUp();
      },
      onDown: () => {
        // Socket already gone: stop sending, then drop everything that was held.
        this.engine.detach();
        this.engine.releaseAll();
        this.camera.onConnectionDown();
      },
      onTracking: (msg) => {
        this.camera.onTracking(msg);
      },
      onCalibration: (msg) => {
        this.camera.onCalibration(msg);
      },
    });
    const connection = this.connection;
    this.camera = new CameraPointer({
      sink: connection,
      sendGyro: (msg) => connection.send(msg),
      sendControl: (msg) => connection.send(msg),
      ...(config.createCamera ? { createCamera: config.createCamera } : {}),
    });
    this.motion = new MotionSensor('face', {
      ...config.motion,
      onSample: () => {
        this.engine.motionChanged();
      },
    });
    this.engine.setMotionSource(this.motion.getLatest);
    // Camera pointing needs the back of the phone toward the TV ("photo grip"); otherwise the phone is held like
    // a Wii Remote, screen up and top edge toward the TV.
    this.camera.subscribe(() => {
      const s = this.camera.getState().status;
      this.motion.setGrip(s === 'on' || s === 'starting' ? 'upright' : 'face');
    });
  }

  start(): void {
    this.unbindPage ??= bindPageLifecycle({
      connection: this.connection,
      releaseForBackground: () => {
        this.releaseForBackground();
      },
      onHidden: () => {
        this.camera.suspend();
        this.resumeMotion = this.motion.getStatus() === 'on';
        this.motion.disable();
      },
      onVisible: () => {
        void this.camera.resume();
        if (this.resumeMotion) void this.motion.enable();
        this.resumeMotion = false;
      },
    });
    this.connection.start();
  }

  /** Release everything (sending one final empty snapshot if possible) and close. */
  dispose(): void {
    this.unbindPage?.();
    this.unbindPage = null;
    this.camera.disable();
    this.motion.disable();
    this.engine.releaseAllAndFlush();
    this.engine.detach();
    this.connection.stop();
  }

  /** Page hidden / window blurred: never leave a button held while the user can't see it. */
  releaseForBackground(): void {
    this.engine.releaseAllAndFlush();
  }
}
