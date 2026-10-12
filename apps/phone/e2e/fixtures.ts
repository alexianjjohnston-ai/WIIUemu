// Test fixtures: a mocked PC server (page.routeWebSocket) and real multi-touch via CDP.

import { test as base, expect, type CDPSession, type Locator, type Page, type WebSocketRoute } from '@playwright/test';
import { validateClientFrame } from '../src/protocol/validate';
import type { HelloMessage, InputMessage } from '../src/protocol/types';

export interface WelcomeOptions {
  slot: number;
  player: number;
}

export interface Received {
  conn: number;
  at: number;
}

/** In-page fake of the Rust server. Every frame the phone sends is checked against the protocol. */
export class MockServer {
  connections = 0;
  hellos: (HelloMessage & Received)[] = [];
  inputs: (InputMessage & Received)[] = [];
  violations: string[] = [];
  current: WebSocketRoute | null = null;
  /** Reply to hello with welcome automatically (set to null to answer manually). */
  autoWelcome: WelcomeOptions | null = { slot: 1, player: 1 };
  /** Simulate the PC being unreachable: new sockets are closed immediately. */
  refuseConnections = false;

  async install(page: Page): Promise<void> {
    await page.routeWebSocket(/\/ws$/, (ws) => {
      this.connections += 1;
      const conn = this.connections;
      if (this.refuseConnections) {
        void ws.close({ code: 1011, reason: 'refused' });
        return;
      }
      this.current = ws;
      ws.onMessage((frame) => {
        const text = typeof frame === 'string' ? frame : frame.toString('utf8');
        for (const e of validateClientFrame(text, 'wiimote')) this.violations.push(`${e} in ${text}`);
        const msg = JSON.parse(text) as { type: string; id?: number };
        const meta = { conn, at: Date.now() };
        switch (msg.type) {
          case 'hello':
            this.hellos.push({ ...(msg as HelloMessage), ...meta });
            if (this.autoWelcome) this.welcome(this.autoWelcome, ws);
            break;
          case 'input':
            this.inputs.push({ ...(msg as InputMessage), ...meta });
            break;
          case 'ping':
            ws.send(JSON.stringify({ type: 'pong', id: msg.id }));
            break;
        }
      });
    });
  }

  welcome(opts: WelcomeOptions, ws: WebSocketRoute | null = this.current): void {
    ws?.send(JSON.stringify({ type: 'welcome', protocol: 1, role: 'wiimote', ...opts }));
  }

  sendError(code: string, message: string): void {
    this.current?.send(JSON.stringify({ type: 'error', code, message }));
  }

  async close(code = 1001, reason = 'server going away'): Promise<void> {
    await this.current?.close({ code, reason });
  }

  /** Index to remember "everything after this point". */
  mark(): number {
    return this.inputs.length;
  }

  since(mark: number): (InputMessage & Received)[] {
    return this.inputs.slice(mark);
  }

  /** Wait until a snapshot after `mark` has exactly these buttons (order-insensitive). */
  async expectSnapshot(mark: number, buttons: string[]): Promise<void> {
    const want = [...buttons].sort().join(',');
    await expect
      .poll(() => this.since(mark).some((m) => [...m.buttons].sort().join(',') === want), {
        message: `snapshot with [${want}]`,
        timeout: 3000,
      })
      .toBe(true);
  }
}

/** Real multi-touch: CDP touch events go through Chromium's input pipeline (pointer IDs, capture). */
export class Touch {
  private readonly points = new Map<number, { x: number; y: number }>();
  constructor(private readonly cdp: CDPSession) {}

  private list() {
    return [...this.points].map(([id, p]) => ({ id, x: p.x, y: p.y, radiusX: 4, radiusY: 4, force: 1 }));
  }

  async down(id: number, target: Locator, offset?: { x: number; y: number }): Promise<void> {
    const box = await target.boundingBox();
    if (!box) throw new Error('target not visible');
    const x = box.x + (offset ? offset.x : box.width / 2);
    const y = box.y + (offset ? offset.y : box.height / 2);
    this.points.set(id, { x, y });
    await this.cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: this.list() });
  }

  async moveTo(id: number, x: number, y: number): Promise<void> {
    this.points.set(id, { x, y });
    await this.cdp.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: this.list() });
  }

  async up(id: number): Promise<void> {
    const p = this.points.get(id);
    if (!p) return;
    this.points.delete(id);
    // For touchEnd, CDP's touchPoints are the points being lifted; the others stay down.
    await this.cdp.send('Input.dispatchTouchEvent', {
      type: 'touchEnd',
      touchPoints: [{ id, x: p.x, y: p.y }],
    });
  }

  async cancelAll(): Promise<void> {
    this.points.clear();
    await this.cdp.send('Input.dispatchTouchEvent', { type: 'touchCancel', touchPoints: [] });
  }
}

export const DEVICE_ID_RE = /^[A-Za-z0-9-]{8,64}$/;

export const test = base.extend<{ server: MockServer; touch: Touch }>({
  // Auto: every test talks to the mock (never a real server), even if it doesn't use `server`.
  server: [
    async ({ page }, use) => {
      const server = new MockServer();
      await server.install(page);
      await use(server);
      expect(server.violations, 'every client frame must satisfy the protocol').toEqual([]);
    },
    { auto: true },
  ],
  touch: async ({ page }, use) => {
    const cdp = await page.context().newCDPSession(page);
    await use(new Touch(cdp));
  },
});

export { expect };

/** Open the app with a token and connect as a Wii Remote. */
export async function connectAsWiimote(page: Page, token = 'e2e-token-123'): Promise<void> {
  await page.goto(`/?token=${encodeURIComponent(token)}`);
  await page.getByRole('button', { name: 'Connect' }).click();
  await expect(page.getByTestId('wiimote-screen')).toBeVisible();
}

/** Preload layout settings before the app reads them. */
export async function presetSettings(
  page: Page,
  settings: { layout: 'authentic' | 'comfort'; leftHanded: boolean; scale: number },
): Promise<void> {
  await page.addInitScript((s) => {
    localStorage.setItem('wiiu-connect.settings.v1', JSON.stringify(s));
  }, settings);
}
