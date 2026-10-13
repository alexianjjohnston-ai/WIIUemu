import type { Page } from '@playwright/test';
import { connectAsGamepad, connectAsWiimote, expect, test } from './fixtures';

declare global {
  interface Window {
    __buzz: (number | number[])[];
  }
}

/** Android-style phone: record `navigator.vibrate` calls. */
async function recordVibrate(page: Page): Promise<void> {
  await page.addInitScript(() => {
    window.__buzz = [];
    Object.defineProperty(navigator, 'vibrate', {
      configurable: true,
      value: (p: number | number[]) => {
        window.__buzz.push(p);
        return true;
      },
    });
  });
}

const buzzes = (page: Page) => page.evaluate(() => window.__buzz);

async function pickVibration(page: Page, label: 'Off' | 'Light' | 'Strong') {
  await page.getByRole('button', { name: 'Settings' }).click();
  await page.getByTestId('haptics-setting').getByText(label, { exact: true }).click();
  await page.getByRole('button', { name: 'Done' }).click();
}

test.describe('haptics', () => {
  test('Wii Remote: buzzes on connect and presses, follows the setting, buzzes when the link drops', async ({
    page,
    server,
    touch,
  }) => {
    await recordVibrate(page);
    await connectAsWiimote(page);
    await expect.poll(() => buzzes(page)).toEqual([[12, 80, 12]]);

    await touch.down(1, page.getByTestId('btn-A'));
    await touch.up(1);
    await expect.poll(() => buzzes(page)).toEqual([[12, 80, 12], 8]);

    await pickVibration(page, 'Off');
    await touch.down(1, page.getByTestId('btn-A'));
    await touch.up(1);
    await pickVibration(page, 'Strong');
    await touch.down(1, page.getByTestId('btn-A'));
    await touch.up(1);
    await expect.poll(() => buzzes(page)).toEqual([[12, 80, 12], 8, 18]);

    // The setting survives a reload: the next connect buzzes strong (the init script clears the record).
    await page.reload();
    await page.getByRole('button', { name: 'Connect' }).click();
    await expect.poll(() => buzzes(page)).toEqual([[25, 80, 25]]);

    await server.close();
    await expect.poll(() => buzzes(page)).toContainEqual([60, 60, 60, 60, 60]);
  });

  test('a phone without the Vibration API (iPhone) says so instead of offering the setting', async ({ page }) => {
    await page.addInitScript(() => {
      Object.defineProperty(navigator, 'vibrate', { configurable: true, value: undefined });
    });
    await connectAsWiimote(page);
    await page.getByRole('button', { name: 'Settings' }).click();
    const setting = page.getByTestId('haptics-setting');
    await expect(setting).toHaveAttribute('data-supported', 'false');
    await expect(setting).toContainText("can't vibrate");
    await expect(setting.getByRole('radio')).toHaveCount(0);
  });

  test('GamePad: the menu has the setting, and a stick buzzes once at its rim', async ({ page, touch }) => {
    await recordVibrate(page);
    await connectAsGamepad(page);
    await expect.poll(() => buzzes(page)).toEqual([[12, 80, 12]]);

    const stick = page.getByTestId('stick');
    const box = await stick.boundingBox();
    if (!box) throw new Error('stick not visible');
    const cx = box.x + box.width / 2;
    const cy = box.y + box.height / 2;
    await touch.down(1, stick);
    await touch.moveTo(1, cx + box.width, cy);
    await touch.moveTo(1, cx + box.width, cy + 4);
    await expect.poll(() => buzzes(page)).toEqual([[12, 80, 12], 6]);
    await touch.up(1);

    await page.getByRole('button', { name: 'Settings' }).click();
    await expect(page.getByTestId('haptics-setting')).toBeVisible();
    await expect(page.getByTestId('haptics-setting').getByRole('radio', { name: 'Light' })).toBeChecked();
  });
});
