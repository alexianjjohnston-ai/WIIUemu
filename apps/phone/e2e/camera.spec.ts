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
    expect(Math.max(f?.width ?? 0, f?.height ?? 0)).toBeLessThanOrEqual(960);
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
    // iOS user agent = WebKit, which reports CoreMotion x/y/z as alpha/beta/gamma (docs/research.md §9).
    const ios = await page.evaluate(() => /iPhone|iPad|iPod/.test(navigator.userAgent));
    const [x, y, z] = ios ? [Math.PI / 2, Math.PI, -Math.PI / 4] : [Math.PI, -Math.PI / 4, Math.PI / 2];
    expect(sample?.[1]).toBeCloseTo(x); // W3C: beta → x
    expect(sample?.[2]).toBeCloseTo(y); // W3C: gamma → y
    expect(sample?.[3]).toBeCloseTo(z); // W3C: alpha → z
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

  test('calibrates the pointer and re-applies the stored calibration after reopening', async ({ page, server }) => {
    await connectAsWiimote(page);
    await page.getByTestId('camera-toggle').click();
    await expect(page.getByTestId('camera-status')).toHaveAttribute('data-kind', 'waiting');
    const row = page.getByTestId('calibration');
    // Until the PC sees the border, calibrating can't work: nothing is offered.
    await expect(row).toHaveCount(0);

    server.sendTracking({ state: 'tracking', pointer: { x: 0.4, y: 0.6 }, tags: 9, fixes_per_s: 29 });
    await expect(row).toHaveAttribute('data-status', 'none');
    await page.getByTestId('calibrate').click();
    // A countdown gives time to aim after tapping; then the phone asks the PC to measure.
    await expect(row).toHaveText('Aim at the centre cross · 3');
    await expect(page.getByTestId('calibrate')).toHaveCount(0);
    expect(server.calibrateRequests.length).toBe(0);
    await expect.poll(() => server.calibrateRequests.length, { timeout: 5000 }).toBe(1);
    await expect(row).toHaveAttribute('data-status', 'measuring');

    // A failure says what to do and offers another try.
    server.sendCalibration({ ok: false, point: null, reason: 'off_image' });
    await expect(row).toHaveAttribute('data-status', 'failed');
    await expect(row).toContainText('Aim at the cross');
    await page.getByTestId('calibrate').click();
    await expect.poll(() => server.calibrateRequests.length, { timeout: 5000 }).toBe(2);
    server.sendCalibration({ ok: true, point: { x: 0.58, y: 0.46 }, reason: null });
    await expect(row).toHaveText('Calibrated ✓');
    await expect(page.getByTestId('calibrate')).toHaveText('Recalibrate', { timeout: 5000 });
    expect(server.violations).toEqual([]);

    // The phone keeps its calibration: after reopening the page it sends it as soon as the camera is on again.
    await connectAsWiimote(page);
    await expect(page.getByTestId('led-1')).toHaveAttribute('data-lit', 'true');
    await page.getByTestId('camera-toggle').click();
    await expect.poll(() => server.aims.at(-1)?.point ?? null).toEqual({ x: 0.58, y: 0.46 });
    await expect(page.getByTestId('camera-status')).toHaveAttribute('data-kind', 'waiting');
    server.sendTracking({ state: 'tracking', pointer: { x: 0.5, y: 0.5 }, tags: 9, fixes_per_s: 29 });
    await expect(row).toHaveAttribute('data-status', 'saved');

    // Reset goes back to the camera image centre.
    await page.getByTestId('calibration-reset').click();
    await expect.poll(() => server.aims.at(-1)?.point).toBeNull();
    await expect(row).toHaveAttribute('data-status', 'none');
    expect(server.violations).toEqual([]);
  });
});
