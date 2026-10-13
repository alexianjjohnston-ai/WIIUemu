import type { Page } from '@playwright/test';
import { connectAsGamepad, connectAsWiimote, expect, test } from './fixtures';

declare global {
  interface Window {
    __buzz: (number | number[])[];
    __ticks: number;
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

  test('iPhone without the Vibration API: clicks a hidden switch, one tick per pulse', async ({ page, touch }) => {
    await page.addInitScript(() => {
      window.__ticks = 0;
      Object.defineProperty(navigator, 'vibrate', { configurable: true, value: undefined });
      // Safari 17.4+ has the `switch` attribute on checkboxes; Chromium doesn't, so pretend.
      Object.defineProperty(HTMLInputElement.prototype, 'switch', { configurable: true, value: false });
      const proto = HTMLLabelElement.prototype;
      const click = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'click')?.value as (this: HTMLElement) => void;
      proto.click = function (this: HTMLLabelElement) {
        if (this.querySelector('input[switch]')) window.__ticks += 1;
        click.call(this);
      };
    });
    await connectAsWiimote(page);
    // "connected" is two pulses: two ticks.
    await expect.poll(() => page.evaluate(() => window.__ticks)).toBe(2);
    await touch.down(1, page.getByTestId('btn-A'));
    await touch.up(1);
    await expect.poll(() => page.evaluate(() => window.__ticks)).toBe(3);
    // The hidden switch never shows and never takes focus.
    await expect(page.locator('label:has(input[switch])')).toBeHidden();
    await expect(page.getByTestId('btn-A')).toBeVisible();
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
