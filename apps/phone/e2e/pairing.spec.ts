import { connectAsWiimote, DEVICE_ID_RE, expect, test } from './fixtures';

test.describe('pairing and connection', () => {
  test('sends hello with the token from the URL', async ({ page, server }) => {
    await page.goto('/?token=qr-token-abc');
    await expect(page.getByTestId('token-from-url')).toBeVisible();
    await page.getByRole('button', { name: 'Connect' }).click();

    await expect.poll(() => server.hellos.length).toBe(1);
    const hello = server.hellos[0];
    expect(hello).toMatchObject({ type: 'hello', protocol: 1, token: 'qr-token-abc', role: 'wiimote' });
    expect(hello?.deviceId).toMatch(DEVICE_ID_RE);
  });

  test('keeps the same deviceId across reloads', async ({ page, server }) => {
    await connectAsWiimote(page);
    await expect.poll(() => server.hellos.length).toBe(1);
    await page.reload();
    await page.getByRole('button', { name: 'Connect' }).click();
    await expect.poll(() => server.hellos.length).toBe(2);
    expect(server.hellos[1]?.deviceId).toBe(server.hellos[0]?.deviceId);
  });

  test('allows manual token entry when the URL has none', async ({ page, server }) => {
    await page.goto('/');
    const connect = page.getByRole('button', { name: 'Connect' });
    await expect(connect).toBeDisabled();
    await page.getByLabel('Pairing code').fill('  typed-token  ');
    await connect.click();
    await expect.poll(() => server.hellos[0]?.token).toBe('typed-token');
  });

  test('welcome lights the LED of the assigned player', async ({ page, server }) => {
    server.autoWelcome = { slot: 2, player: 2 };
    await connectAsWiimote(page);
    await expect(page.getByTestId('led-2')).toHaveAttribute('data-lit', 'true');
    for (const n of [1, 3, 4]) await expect(page.getByTestId(`led-${n}`)).toHaveAttribute('data-lit', 'false');
    await expect(page.getByTestId('status')).toContainText('Player 2');
    await expect(page.getByTestId('connection-notice')).toHaveCount(0);
  });

  test('shows connecting state until welcome arrives', async ({ page, server }) => {
    server.autoWelcome = null;
    await connectAsWiimote(page);
    await expect(page.getByTestId('connection-notice')).toHaveAttribute('data-kind', 'connecting');
    for (const n of [1, 2, 3, 4]) await expect(page.getByTestId(`led-${n}`)).toHaveAttribute('data-lit', 'false');
    await expect.poll(() => server.hellos.length).toBe(1);
    // No input before the handshake completes.
    expect(server.inputs).toHaveLength(0);
    server.welcome({ slot: 1, player: 1 });
    await expect(page.getByTestId('led-1')).toHaveAttribute('data-lit', 'true');
  });

  test('server closing the socket shows the disconnected state, then reconnects', async ({ page, server }) => {
    await connectAsWiimote(page);
    await expect(page.getByTestId('led-1')).toHaveAttribute('data-lit', 'true');

    await server.close();
    const notice = page.getByTestId('connection-notice');
    await expect(notice).toHaveAttribute('data-kind', 'reconnecting');
    await expect(notice).toContainText('Connection to the PC lost');
    await expect(page.getByTestId('status')).toContainText('Disconnected');
    await expect(page.getByTestId('led-1')).toHaveAttribute('data-lit', 'false');

    // Automatic reconnect (first backoff step is ~0.5 s) with the same identity.
    await expect.poll(() => server.connections, { timeout: 5000 }).toBe(2);
    await expect.poll(() => server.hellos.length).toBe(2);
    expect(server.hellos[1]?.deviceId).toBe(server.hellos[0]?.deviceId);
    expect(server.hellos[1]?.token).toBe('e2e-token-123');
    await expect(notice).toHaveCount(0);
    await expect(page.getByTestId('led-1')).toHaveAttribute('data-lit', 'true');
  });

  test('a rejected token shows a readable error and stops retrying', async ({ page, server }) => {
    server.autoWelcome = null;
    await connectAsWiimote(page);
    await expect.poll(() => server.hellos.length).toBe(1);
    server.sendError('bad_token', 'pairing token rejected');
    await server.close(1008, 'bad token');

    const notice = page.getByTestId('connection-notice');
    await expect(notice).toHaveAttribute('data-kind', 'failed');
    await expect(notice).toContainText('Pairing code rejected');
    await page.waitForTimeout(1500);
    expect(server.connections).toBe(1);

    // "Back" returns to the connect screen where the code can be changed.
    await notice.getByRole('button', { name: 'Back' }).click();
    await expect(page.getByTestId('connect-screen')).toBeVisible();
  });

  test('role_unavailable explains that all slots are taken', async ({ page, server }) => {
    server.autoWelcome = null;
    await connectAsWiimote(page);
    await expect.poll(() => server.hellos.length).toBe(1);
    server.sendError('role_unavailable', 'all wiimote slots are taken');
    await server.close(1008);
    await expect(page.getByTestId('connection-notice')).toContainText('No free controller slot');
  });

  test('GamePad role shows the coming-soon placeholder without connecting', async ({ page, server }) => {
    await page.goto('/?token=abc');
    await page.getByText('GamePad').click();
    await page.getByRole('button', { name: 'Continue' }).click();
    await expect(page.getByTestId('gamepad-placeholder')).toBeVisible();
    expect(server.connections).toBe(0);
    await page.getByRole('button', { name: 'Back' }).click();
    await expect(page.getByTestId('connect-screen')).toBeVisible();
  });
});
