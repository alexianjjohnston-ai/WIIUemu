import { connectAsWiimote, expect, test } from './fixtures';

test.describe('Wii Remote input', () => {
  test.beforeEach(async ({ page }) => {
    await connectAsWiimote(page);
    await expect(page.getByTestId('led-1')).toHaveAttribute('data-lit', 'true');
  });

  test('pressing A sends a snapshot with A, releasing sends one without it', async ({ page, server, touch }) => {
    const a = page.getByTestId('btn-A');
    const start = server.mark();
    await touch.down(1, a);
    await server.expectSnapshot(start, ['A']);
    await expect(a).toHaveAttribute('data-pressed', 'true');

    const afterPress = server.mark();
    await touch.up(1);
    await server.expectSnapshot(afterPress, []);
    await expect(a).toHaveAttribute('data-pressed', 'false');

    // Once released, A never comes back in later snapshots.
    const lastWithA = server.inputs.findLastIndex((m) => m.buttons.includes('A'));
    expect(server.inputs.slice(lastWithA + 1).every((m) => m.buttons.length === 0)).toBe(true);
  });

  test('holding B while pressing A sends both', async ({ page, server, touch }) => {
    const start = server.mark();
    await touch.down(1, page.getByTestId('btn-B'));
    await server.expectSnapshot(start, ['B']);
    await touch.down(2, page.getByTestId('btn-A'));
    await server.expectSnapshot(start, ['A', 'B']);
    await expect(page.getByTestId('btn-A')).toHaveAttribute('data-pressed', 'true');
    await expect(page.getByTestId('btn-B')).toHaveAttribute('data-pressed', 'true');

    // Lift the thumb: B stays held.
    const afterBoth = server.mark();
    await touch.up(2);
    await server.expectSnapshot(afterBoth, ['B']);
    await expect(page.getByTestId('btn-B')).toHaveAttribute('data-pressed', 'true');

    const afterA = server.mark();
    await touch.up(1);
    await server.expectSnapshot(afterA, []);
  });

  test('holding B while using the D-pad sends both', async ({ page, server, touch }) => {
    const start = server.mark();
    const dpad = page.getByTestId('dpad');
    const box = await dpad.boundingBox();
    if (!box) throw new Error('no dpad');
    await touch.down(1, page.getByTestId('btn-B'));
    // Touch the upper arm of the D-pad, then slide to the right arm.
    await touch.down(2, dpad, { x: box.width / 2, y: box.height * 0.15 });
    await server.expectSnapshot(start, ['B', 'UP']);
    await expect(page.getByTestId('btn-UP')).toHaveAttribute('data-pressed', 'true');
    await touch.moveTo(2, box.x + box.width * 0.85, box.y + box.height / 2);
    await server.expectSnapshot(start, ['B', 'RIGHT']);
    await touch.cancelAll();
    await server.expectSnapshot(start, []);
  });

  test('a cancelled touch releases its button', async ({ page, server, touch }) => {
    const start = server.mark();
    await touch.down(1, page.getByTestId('btn-HOME'));
    await server.expectSnapshot(start, ['HOME']);
    const mid = server.mark();
    await touch.cancelAll();
    await server.expectSnapshot(mid, []);
    await expect(page.getByTestId('btn-HOME')).toHaveAttribute('data-pressed', 'false');
  });

  test('every button maps to its protocol name', async ({ page, server, touch }) => {
    for (const b of ['A', 'B', 'ONE', 'TWO', 'PLUS', 'MINUS', 'HOME']) {
      const start = server.mark();
      await touch.down(1, page.getByTestId(`btn-${b}`));
      await server.expectSnapshot(start, [b]);
      await touch.up(1);
      await server.expectSnapshot(start, []);
    }
  });

  test('sends heartbeats at least every 100 ms with increasing seq', async ({ server }) => {
    const start = server.mark();
    await expect.poll(() => server.since(start).length, { timeout: 3000 }).toBeGreaterThanOrEqual(15);
    const msgs = server.inputs.filter((m) => m.conn === 1);
    for (let i = 1; i < msgs.length; i++) {
      expect(msgs[i]?.seq).toBeGreaterThan(msgs[i - 1]?.seq ?? Infinity);
    }
    // Time measured on the test side includes IPC jitter; allow generous slack over the 50 ms target.
    const gaps = msgs.slice(1).map((m, i) => m.at - (msgs[i]?.at ?? 0));
    expect(Math.max(...gaps)).toBeLessThan(100);
    expect(msgs.every((m) => m.pointer === null)).toBe(true);
  });

  test('hiding the page releases everything with a final empty snapshot', async ({ page, server, touch }) => {
    const start = server.mark();
    await touch.down(1, page.getByTestId('btn-A'));
    await server.expectSnapshot(start, ['A']);
    const mid = server.mark();
    await page.evaluate(() => {
      Object.defineProperty(document, 'visibilityState', { value: 'hidden', configurable: true });
      document.dispatchEvent(new Event('visibilitychange'));
    });
    await server.expectSnapshot(mid, []);
    await expect(page.getByTestId('btn-A')).toHaveAttribute('data-pressed', 'false');
    await touch.up(1);
  });

  test('a server disconnect releases held buttons', async ({ page, server, touch }) => {
    await touch.down(1, page.getByTestId('btn-A'));
    await expect(page.getByTestId('btn-A')).toHaveAttribute('data-pressed', 'true');
    await server.close();
    await expect(page.getByTestId('btn-A')).toHaveAttribute('data-pressed', 'false');
    // After reconnecting, the first snapshot on the new socket is empty.
    await expect.poll(() => server.inputs.some((m) => m.conn === 2)).toBe(true);
    expect(server.inputs.find((m) => m.conn === 2)?.buttons).toEqual([]);
    await touch.up(1);
  });
});
