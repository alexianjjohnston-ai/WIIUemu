import { connectAsWiimote, expect, presetSettings, test } from './fixtures';
import type { Locator, Page } from '@playwright/test';

const CONTROLS = ['dpad', 'btn-A', 'btn-B', 'btn-MINUS', 'btn-HOME', 'btn-PLUS', 'btn-ONE', 'btn-TWO', 'player-leds'];

async function expectFitsViewport(page: Page) {
  const vp = page.viewportSize();
  if (!vp) throw new Error('no viewport');
  const metrics = await page.evaluate(() => {
    const inner = document.querySelector('.wm-remote-inner');
    return {
      docScroll: document.documentElement.scrollHeight - window.innerHeight,
      docScrollX: document.documentElement.scrollWidth - window.innerWidth,
      innerOverflow: inner ? inner.scrollHeight - inner.clientHeight : -1,
    };
  });
  expect(metrics.docScroll, 'page must not scroll vertically').toBeLessThanOrEqual(0);
  expect(metrics.docScrollX, 'page must not scroll horizontally').toBeLessThanOrEqual(0);
  expect(metrics.innerOverflow, 'remote content must not overflow the body').toBeLessThanOrEqual(1);

  const boxes: ({ id: string } & NonNullable<Awaited<ReturnType<Locator['boundingBox']>>>)[] = [];
  for (const id of CONTROLS) {
    const box = await page.getByTestId(id).boundingBox();
    expect(box, id).not.toBeNull();
    if (!box) continue;
    expect(box.x, `${id} left`).toBeGreaterThanOrEqual(0);
    expect(box.y, `${id} top`).toBeGreaterThanOrEqual(0);
    expect(box.x + box.width, `${id} right`).toBeLessThanOrEqual(vp.width + 0.5);
    expect(box.y + box.height, `${id} bottom`).toBeLessThanOrEqual(vp.height + 0.5);
    boxes.push({ id, ...box });
  }
  // Order top to bottom: LEDs, D-pad, A, -/HOME/+, 1, 2.
  const y = (id: string) => boxes.find((b) => b.id === id)?.y ?? NaN;
  expect(y('player-leds')).toBeLessThan(y('dpad'));
  expect(y('dpad')).toBeLessThan(y('btn-A'));
  expect(y('btn-A')).toBeLessThan(y('btn-HOME'));
  expect(y('btn-HOME')).toBeLessThan(y('btn-ONE'));
  expect(y('btn-ONE')).toBeLessThan(y('btn-TWO'));
  // Hit areas must not overlap (except the B zone, which is beside the remote).
  const remoteControls = boxes.filter((b) => b.id !== 'btn-B' && b.id !== 'player-leds');
  for (let i = 0; i < remoteControls.length; i++) {
    for (let j = i + 1; j < remoteControls.length; j++) {
      const a = remoteControls[i];
      const b = remoteControls[j];
      if (!a || !b) continue;
      const overlap =
        a.x < b.x + b.width - 0.5 &&
        b.x < a.x + a.width - 0.5 &&
        a.y < b.y + b.height - 0.5 &&
        b.y < a.y + a.height - 0.5;
      expect(overlap, `${a.id} overlaps ${b.id}`).toBe(false);
    }
  }
}

for (const viewport of [
  { width: 360, height: 740 },
  { width: 390, height: 844 },
]) {
  for (const layout of ['authentic', 'comfort'] as const) {
    for (const scale of [0.8, 1, 1.4]) {
      test(`${layout} layout at ${scale}x fits ${viewport.width}x${viewport.height} without scrolling`, async ({
        page,
      }) => {
        await page.setViewportSize(viewport);
        await presetSettings(page, { layout, leftHanded: false, scale });
        await connectAsWiimote(page);
        await expect(page.getByTestId('led-1')).toHaveAttribute('data-lit', 'true');
        await expectFitsViewport(page);
      });
    }
  }
}

test('left-handed mode moves the B zone to the left edge (and persists)', async ({ page }) => {
  await connectAsWiimote(page);
  const b = page.getByTestId('btn-B');
  const remote = page.getByTestId('remote');
  const vp = page.viewportSize();
  if (!vp) throw new Error('no viewport');

  let bBox = await b.boundingBox();
  let rBox = await remote.boundingBox();
  expect(bBox && rBox && bBox.x > rBox.x + rBox.width).toBe(true);
  expect(bBox && bBox.x + bBox.width > vp.width - 40).toBe(true);
  await expect(b).toHaveAttribute('data-side', 'right');

  await page.getByRole('button', { name: 'Settings' }).click();
  await page.getByLabel(/Left-handed/).check();
  await page.getByRole('button', { name: 'Done' }).click();

  await expect(b).toHaveAttribute('data-side', 'left');
  bBox = await b.boundingBox();
  rBox = await remote.boundingBox();
  expect(bBox && rBox && bBox.x + bBox.width < rBox.x).toBe(true);
  expect(bBox && bBox.x < 40).toBe(true);

  // Persisted across reloads.
  await page.reload();
  await page.getByRole('button', { name: 'Connect' }).click();
  await expect(page.getByTestId('btn-B')).toHaveAttribute('data-side', 'left');
});

test('settings change layout and button size', async ({ page }) => {
  await connectAsWiimote(page);
  const screen = page.getByTestId('wiimote-screen');
  await expect(screen).toHaveAttribute('data-layout', 'authentic');
  const bAuthentic = (await page.getByTestId('btn-B').boundingBox())?.width ?? 0;
  const aSmall = (await page.getByTestId('btn-A').boundingBox())?.width ?? 0;

  await page.getByRole('button', { name: 'Settings' }).click();
  await page.getByText('Comfort', { exact: true }).click();
  await page.getByLabel(/Button size/).fill('1.4');
  await page.getByRole('button', { name: 'Done' }).click();

  await expect(screen).toHaveAttribute('data-layout', 'comfort');
  const bComfort = (await page.getByTestId('btn-B').boundingBox())?.width ?? 0;
  const aBig = (await page.getByTestId('btn-A').boundingBox())?.width ?? 0;
  expect(bComfort).toBeGreaterThan(bAuthentic);
  expect(aBig).toBeGreaterThan(aSmall);
});

/** Boxes of the camera overlays (status line and calibration row), which float above the remote. */
async function overlayBoxes(page: Page) {
  const out = [];
  for (const id of ['camera-status', 'calibration']) {
    const box = await page.getByTestId(id).boundingBox();
    if (box) out.push({ id, ...box });
  }
  return out;
}

for (const viewport of [
  { width: 360, height: 740 },
  { width: 390, height: 844 },
]) {
  for (const [layout, leftHanded] of [
    ['authentic', false],
    ['comfort', false],
    ['authentic', true],
  ] as const) {
    const hand = leftHanded ? ', left-handed' : '';
    test(`camera overlays never cover a control (${layout}${hand}, ${viewport.width}x${viewport.height})`, async ({
      page,
      server,
    }) => {
      await page.setViewportSize(viewport);
      await presetSettings(page, { layout, leftHanded, scale: 1 });
      await connectAsWiimote(page);
      await page.getByTestId('camera-toggle').click();
      await expect(page.getByTestId('camera-status')).toHaveAttribute('data-kind', 'waiting');
      const states: [string, () => void | Promise<void>][] = [
        [
          'offered',
          () => {
            server.sendTracking({ state: 'tracking', pointer: null, tags: 9, fixes_per_s: 29 });
          },
        ],
        ['countdown', () => page.getByTestId('calibrate').click()],
        [
          'failed',
          () => {
            server.sendCalibration({ ok: false, point: null, reason: 'no_markers' });
          },
        ],
        [
          'saved',
          async () => {
            await page.getByTestId('calibrate').click();
            await expect.poll(() => server.calibrateRequests.length, { timeout: 5000 }).toBe(2);
            server.sendCalibration({ ok: true, point: { x: 0.5, y: 0.5 }, reason: null });
            await expect(page.getByTestId('calibration-reset')).toBeVisible({ timeout: 5000 });
          },
        ],
      ];
      for (const [name, enter] of states) {
        await enter();
        await expect(page.getByTestId('calibration')).toBeVisible();
        for (const o of await overlayBoxes(page)) {
          // The player LEDs only display the player number (also shown in the status bar): not a control.
          for (const id of CONTROLS.filter((c) => c !== 'player-leds')) {
            const c = await page.getByTestId(id).boundingBox();
            if (!c) continue;
            const overlap =
              o.x < c.x + c.width - 0.5 &&
              c.x < o.x + o.width - 0.5 &&
              o.y < c.y + c.height - 0.5 &&
              c.y < o.y + o.height - 0.5;
            expect(overlap, `${name}: ${o.id} covers ${id}`).toBe(false);
          }
        }
      }
    });
  }
}
