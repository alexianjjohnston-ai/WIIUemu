import { connectAsGamepad, expect, test, type MockServer } from './fixtures';
import type { Page } from '@playwright/test';

async function centreOf(page: Page, testId: string) {
  const box = await page.getByTestId(testId).boundingBox();
  if (!box) throw new Error(`${testId} not visible`);
  return { x: box.x + box.width / 2, y: box.y + box.height / 2, w: box.width, h: box.height };
}

/** Wait for a snapshot after `mark` that satisfies `pred`. */
async function expectInput(
  server: MockServer,
  mark: number,
  what: string,
  pred: (m: MockServer['inputs'][number]) => boolean,
) {
  await expect.poll(() => server.since(mark).some(pred), { message: what, timeout: 3000 }).toBe(true);
}

test.describe('GamePad', () => {
  test.beforeEach(async ({ page }) => {
    await connectAsGamepad(page);
    await expect(page.getByTestId('status')).toContainText('GamePad · Connected');
  });

  test('connects with the gamepad role and sends both sticks, centred', async ({ server }) => {
    expect(server.hellos.at(-1)?.role).toBe('gamepad');
    await expect.poll(() => server.inputs.length).toBeGreaterThan(0);
    expect(server.inputs[0]).toMatchObject({
      buttons: [],
      pointer: null,
      stick: { x: 0, y: 0 },
      rstick: { x: 0, y: 0 },
    });
  });

  test('face and shoulder buttons are pressed together and released', async ({ page, server, touch }) => {
    const start = server.mark();
    await touch.down(1, page.getByTestId('btn-ZR'));
    await touch.down(2, page.getByTestId('btn-A'));
    await server.expectSnapshot(start, ['A', 'ZR']);
    await expect(page.getByTestId('btn-A')).toHaveAttribute('data-pressed', 'true');
    const mid = server.mark();
    await touch.up(2);
    await server.expectSnapshot(mid, ['ZR']);
    await touch.up(1);
    await server.expectSnapshot(mid, []);
    for (const b of ['B', 'X', 'Y', 'L', 'ZL', 'R', 'MINUS', 'PLUS', 'HOME', 'L3', 'R3']) {
      const m = server.mark();
      await touch.down(1, page.getByTestId(`btn-${b}`));
      await server.expectSnapshot(m, [b]);
      await touch.up(1);
      await server.expectSnapshot(m, []);
    }
  });

  test('the D-pad works on the GamePad too', async ({ page, server, touch }) => {
    const dpad = page.getByTestId('dpad');
    const box = await dpad.boundingBox();
    if (!box) throw new Error('no dpad');
    const start = server.mark();
    await touch.down(1, dpad, { x: box.width / 2, y: box.height * 0.15 });
    await server.expectSnapshot(start, ['UP']);
    await touch.up(1);
    await server.expectSnapshot(start, []);
  });

  test('left stick follows the thumb (+y up) and springs back on release', async ({ page, server, touch }) => {
    const c = await centreOf(page, 'stick');
    const start = server.mark();
    await touch.down(1, page.getByTestId('stick'));
    // Drag up and to the right, beyond the rim.
    await touch.moveTo(1, c.x + c.w, c.y - c.h);
    await expectInput(server, start, 'stick up-right', (m) => (m.stick?.x ?? 0) > 0.6 && (m.stick?.y ?? 0) > 0.6);
    await expect(page.getByTestId('stick')).toHaveAttribute('data-held', 'true');
    for (const m of server.since(start))
      expect(Math.hypot(m.stick?.x ?? 0, m.stick?.y ?? 0)).toBeLessThanOrEqual(1.0015);
    const mid = server.mark();
    await touch.up(1);
    await expectInput(server, mid, 'stick centred', (m) => m.stick?.x === 0 && m.stick.y === 0);
    expect(server.inputs.at(-1)?.rstick).toEqual({ x: 0, y: 0 });
  });

  test('right stick and a face button at the same time', async ({ page, server, touch }) => {
    const c = await centreOf(page, 'rstick');
    const start = server.mark();
    await touch.down(1, page.getByTestId('rstick'));
    await touch.moveTo(1, c.x - c.w, c.y);
    await touch.down(2, page.getByTestId('btn-ZL'));
    await expectInput(server, start, 'rstick left + ZL', (m) => (m.rstick?.x ?? 0) < -0.9 && m.buttons.includes('ZL'));
    await touch.cancelAll();
    await expectInput(server, start, 'all released', (m) => m.buttons.length === 0 && m.rstick?.x === 0);
  });

  test('touchscreen sends the normalised touch position and lifts', async ({ page, server, touch }) => {
    const screen = page.getByTestId('touchscreen');
    const box = await screen.boundingBox();
    if (!box) throw new Error('no touchscreen');
    const start = server.mark();
    await touch.down(1, screen, { x: box.width * 0.25, y: box.height * 0.75 });
    await expectInput(
      server,
      start,
      'touch at (0.25, 0.75)',
      (m) => m.pointer !== null && Math.abs(m.pointer.x - 0.25) < 0.02 && Math.abs(m.pointer.y - 0.75) < 0.02,
    );
    await expect(screen).toHaveAttribute('data-touching', 'true');
    await touch.moveTo(1, box.x + box.width * 0.9, box.y + box.height * 0.1);
    await expectInput(
      server,
      start,
      'touch moved',
      (m) => m.pointer !== null && m.pointer.x > 0.85 && m.pointer.y < 0.15,
    );
    const mid = server.mark();
    await touch.up(1);
    await expectInput(server, mid, 'touch lifted', (m) => m.pointer === null);
    await expect(screen).toHaveAttribute('data-touching', 'false');
  });

  test('losing focus releases buttons, sticks and touch', async ({ page, server, touch }) => {
    const c = await centreOf(page, 'stick');
    const start = server.mark();
    await touch.down(1, page.getByTestId('stick'));
    await touch.moveTo(1, c.x, c.y - c.h);
    await touch.down(2, page.getByTestId('btn-A'));
    await touch.down(3, page.getByTestId('touchscreen'));
    await expectInput(
      server,
      start,
      'all held',
      (m) => m.buttons.includes('A') && (m.stick?.y ?? 0) > 0.5 && m.pointer !== null,
    );
    const mid = server.mark();
    await page.evaluate(() => window.dispatchEvent(new Event('blur')));
    await expectInput(
      server,
      mid,
      'released',
      (m) => m.buttons.length === 0 && m.pointer === null && m.stick?.x === 0 && m.stick.y === 0,
    );
  });

  test('every control is on screen and nothing overlaps', async ({ page }) => {
    const ids = [
      'stick',
      'rstick',
      'dpad',
      'touchscreen',
      'btn-A',
      'btn-B',
      'btn-X',
      'btn-Y',
      'btn-L',
      'btn-R',
      'btn-ZL',
      'btn-ZR',
      'btn-HOME',
      'btn-MINUS',
      'btn-PLUS',
      'btn-L3',
      'btn-R3',
    ];
    const vp = page.viewportSize();
    if (!vp) throw new Error('no viewport');
    const boxes = [];
    for (const id of ids) {
      const b = await page.getByTestId(id).boundingBox();
      if (!b) throw new Error(`${id} hidden`);
      expect(b.x, id).toBeGreaterThanOrEqual(0);
      expect(b.y, id).toBeGreaterThanOrEqual(0);
      expect(b.x + b.width, id).toBeLessThanOrEqual(vp.width + 0.5);
      expect(b.y + b.height, id).toBeLessThanOrEqual(vp.height + 0.5);
      expect(Math.min(b.width, b.height), `${id} is big enough to hit`).toBeGreaterThanOrEqual(28);
      boxes.push({ id, ...b });
    }
    for (let i = 0; i < boxes.length; i++) {
      for (let j = i + 1; j < boxes.length; j++) {
        const a = boxes[i];
        const b = boxes[j];
        if (!a || !b) continue;
        const overlap =
          Math.min(a.x + a.width, b.x + b.width) - Math.max(a.x, b.x) > 1 &&
          Math.min(a.y + a.height, b.y + b.height) - Math.max(a.y, b.y) > 1;
        expect(overlap, `${a.id} overlaps ${b.id}`).toBe(false);
      }
    }
    await expect(page.getByTestId('rotate-hint')).toBeHidden();
  });

  test('portrait shows the rotate hint', async ({ page }) => {
    const vp = page.viewportSize();
    if (!vp) throw new Error('no viewport');
    await page.setViewportSize({ width: vp.height, height: vp.width });
    await expect(page.getByTestId('rotate-hint')).toBeVisible();
  });

  test('motion: turning on sends DSU-convention motion from devicemotion events', async ({ page, server }) => {
    // Landscape, top edge to the left.
    await page.evaluate(() => {
      Object.defineProperty(screen.orientation, 'angle', { configurable: true, get: () => 90 });
    });
    await page.getByTestId('motion-toggle').click();
    await expect(page.getByTestId('motion-toggle')).toHaveAttribute('data-state', 'on');
    const ios = await page.evaluate(() => /iPhone|iPad|iPod/.test(navigator.userAgent));
    const start = server.mark();
    // Flat on a table, screen up, turning left at 45 °/s (rotation about device z), in each engine's convention.
    await page.evaluate((webkit) => {
      const g = 9.80665;
      const init = webkit
        ? { accelerationIncludingGravity: { x: 0, y: 0, z: -g }, rotationRate: { alpha: 0, beta: 0, gamma: 45 } }
        : { accelerationIncludingGravity: { x: 0, y: 0, z: g }, rotationRate: { alpha: 45, beta: 0, gamma: 0 } };
      window.dispatchEvent(new DeviceMotionEvent('devicemotion', { ...init, interval: 16 }));
    }, ios);
    await expectInput(
      server,
      start,
      'motion sample',
      (m) => JSON.stringify(m.motion?.accel) === '[0,-1,0]' && JSON.stringify(m.motion?.gyro) === '[0,-45,0]',
    );
    await page.getByTestId('motion-toggle').click();
    await expect(page.getByTestId('motion-toggle')).toHaveAttribute('data-state', 'off');
    const off = server.mark();
    await expect.poll(() => server.since(off).length).toBeGreaterThan(1);
    expect(server.since(off).at(-1)?.motion).toBeUndefined();
  });

  test('disconnect from the menu goes back to the connect screen', async ({ page }) => {
    await page.getByRole('button', { name: 'Settings' }).click();
    await page.getByRole('button', { name: 'Disconnect' }).click();
    await expect(page.getByTestId('connect-screen')).toBeVisible();
  });
});
