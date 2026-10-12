import path from 'node:path';
import { connectAsWiimote, expect, presetSettings, test } from './fixtures';

// Reference screenshots committed to apps/phone/screenshots/ (360x740 at 1x DPR to keep them small).
// Captured on the Pixel 7 project only (see playwright.config.ts).
const DIR = path.resolve(import.meta.dirname, '../screenshots');

test.describe('screenshots', () => {
  test.use({ viewport: { width: 360, height: 740 }, deviceScaleFactor: 1 });

  test('connect screen', async ({ page }) => {
    await page.goto('/?token=abc');
    await expect(page.getByTestId('connect-screen')).toBeVisible();
    await page.screenshot({ path: `${DIR}/connect.png` });
  });

  for (const [name, settings] of [
    ['authentic', { layout: 'authentic', leftHanded: false, scale: 1 }],
    ['comfort', { layout: 'comfort', leftHanded: false, scale: 1 }],
    ['authentic-left-handed', { layout: 'authentic', leftHanded: true, scale: 1 }],
  ] as const) {
    test(`${name} layout`, async ({ page }) => {
      await presetSettings(page, settings);
      await connectAsWiimote(page);
      await expect(page.getByTestId('led-1')).toHaveAttribute('data-lit', 'true');
      await page.screenshot({ path: `${DIR}/${name}.png`, animations: 'disabled' });
    });
  }

  test('reconnecting state', async ({ page, server }) => {
    await connectAsWiimote(page);
    await expect(page.getByTestId('led-1')).toHaveAttribute('data-lit', 'true');
    server.refuseConnections = true;
    await server.close();
    await expect(page.getByTestId('connection-notice')).toHaveAttribute('data-kind', 'reconnecting');
    await page.screenshot({ path: `${DIR}/reconnecting.png`, animations: 'disabled' });
  });
});
