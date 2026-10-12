import { connectAsWiimote, expect, test } from './fixtures';

// Camera pointing (protocol 2) with Chromium's fake camera; see playwright.config.ts.
test.describe('camera pointing', () => {
  test('streams valid JPEG camera frames and shows PC tracking feedback', async ({ page, server }) => {
    await connectAsWiimote(page);
    await expect(page.getByTestId('led-1')).toHaveAttribute('data-lit', 'true');
    const toggle = page.getByTestId('camera-toggle');
    await expect(toggle).toHaveAttribute('aria-pressed', 'false');
    await toggle.click();
    await expect(toggle).toHaveAttribute('aria-pressed', 'true');

    await expect.poll(() => server.frames.length, { timeout: 5000 }).toBeGreaterThan(5);
    const f = server.frames[0];
    expect(f?.jpegMagic).toBe(true);
    expect(Math.max(f?.width ?? 0, f?.height ?? 0)).toBeLessThanOrEqual(640);
    await expect(page.getByTestId('camera-status')).toHaveAttribute('data-kind', 'waiting');

    server.sendTracking({ state: 'tracking', pointer: { x: 0.4, y: 0.6 }, tags: 9, fixes_per_s: 28.6 });
    await expect(page.getByTestId('camera-status')).toHaveText(/Aiming at the TV · 9 tags · 29 fps/);
    await expect(toggle).toHaveAttribute('data-state', 'tracking');
    server.sendTracking({ state: 'lost', pointer: null, tags: 0, fixes_per_s: 0 });
    await expect(page.getByTestId('camera-status')).toHaveAttribute('data-kind', 'lost');

    // Frames are paced (≤ 30 fps target) and never queue up.
    const before = server.frames.length;
    await page.waitForTimeout(1000);
    expect(server.frames.length - before).toBeLessThanOrEqual(35);

    await toggle.click();
    await expect(toggle).toHaveAttribute('aria-pressed', 'false');
    await expect(page.getByTestId('camera-status')).toHaveCount(0);
    const stoppedAt = server.frames.length;
    await page.waitForTimeout(400);
    expect(server.frames.length - stoppedAt).toBeLessThanOrEqual(1);
  });

  test('streams rotation rates as valid gyro batches', async ({ page, server }) => {
    await connectAsWiimote(page);
    await expect(page.getByTestId('led-1')).toHaveAttribute('data-lit', 'true');
    await page.getByTestId('camera-toggle').click();
    await expect(page.getByTestId('camera-status')).toBeVisible();
    await page.evaluate(async () => {
      for (let i = 0; i < 20; i++) {
        window.dispatchEvent(
          new DeviceMotionEvent('devicemotion', { rotationRate: { alpha: 90, beta: 180, gamma: -45 }, interval: 16 }),
        );
        await new Promise((r) => setTimeout(r, 10));
      }
    });
    await expect.poll(() => server.gyro.length).toBeGreaterThan(0);
    const sample = server.gyro[0]?.samples[0];
    expect(sample?.[1]).toBeCloseTo(Math.PI); // beta → x
    expect(sample?.[2]).toBeCloseTo(-Math.PI / 4); // gamma → y
    expect(sample?.[3]).toBeCloseTo(Math.PI / 2); // alpha → z
  });

  test('a protocol 1 PC hides the camera toggle and gets no frames', async ({ page, server }) => {
    server.protocol = 1;
    await connectAsWiimote(page);
    await expect(page.getByTestId('led-1')).toHaveAttribute('data-lit', 'true');
    await expect(page.getByTestId('camera-toggle')).toHaveCount(0);
  });

  test('hiding the page releases the camera', async ({ page, server }) => {
    await connectAsWiimote(page);
    await page.getByTestId('camera-toggle').click();
    await expect.poll(() => server.frames.length).toBeGreaterThan(0);
    await page.evaluate(() => {
      Object.defineProperty(document, 'visibilityState', { value: 'hidden', configurable: true });
      document.dispatchEvent(new Event('visibilitychange'));
    });
    await expect(page.getByTestId('camera-toggle')).toHaveAttribute('aria-pressed', 'false');
    const stoppedAt = server.frames.length;
    await page.waitForTimeout(400);
    expect(server.frames.length - stoppedAt).toBeLessThanOrEqual(1);
  });
});
