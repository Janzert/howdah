// Game windows (docs/WINDOWS.md): in the browser preview a game window is
// a tab opened with `window.open`, on its own session.
import { expect, test } from '@playwright/test';
import type { ArimaaHooks } from '../src/lib/devHooks';

declare global {
  interface Window {
    __arimaa?: ArimaaHooks;
  }
}

test('a second window plays its own game, follows settings, and closing it ends its session', async ({
  page,
  context,
}) => {
  await page.goto('/');
  await page.waitForFunction(() => window.__arimaa?.state() != null);
  await page.evaluate(async () => {
    const a = window.__arimaa!;
    await a.api.setAnalysis(null);
    await a.api.newGame();
    await a.idle();
  });

  const [second] = await Promise.all([
    context.waitForEvent('page'),
    page.getByRole('button', { name: 'New window' }).click(),
  ]);
  await second.waitForFunction(() => window.__arimaa?.state() != null);
  const id = Number(new URL(second.url()).searchParams.get('session'));
  expect(id).toBeGreaterThan(1);
  expect(await page.evaluate(() => window.__arimaa!.api.listSessions())).toContain(id);

  // Both setups and a move in the second window; the first stays in gold's setup.
  await second.evaluate(async () => {
    const a = window.__arimaa!;
    await a.api.commitSetup();
    await a.api.commitSetup();
    await a.idle();
    await a.drag('d2', 'd4', ['d3']);
    await a.api.commitTurn();
    await a.idle();
  });
  await expect(second.getByText('Silver to move')).toBeVisible();
  await expect(page.getByText('Gold setup')).toBeVisible();
  expect(await page.evaluate(() => window.__arimaa!.state()!.ply)).toBe(0);
  expect(await second.evaluate(() => window.__arimaa!.state()!.ply)).toBe(3);

  // A move in the first window doesn't reach the second.
  await page.evaluate(async () => {
    await window.__arimaa!.api.commitSetup();
    await window.__arimaa!.idle();
  });
  await expect(page.getByText('Silver setup')).toBeVisible();
  expect(await second.evaluate(() => window.__arimaa!.state()!.ply)).toBe(3);

  // A setting changed in one window applies in the other.
  const theme = () => page.evaluate(() => document.documentElement.dataset.theme);
  const before = await theme();
  const other = before === 'dark' ? 'Light' : 'Dark';
  await second.getByRole('button', { name: 'Settings' }).click();
  await second.getByRole('dialog', { name: 'Settings' }).getByLabel(other, { exact: true }).check();
  await expect.poll(theme).toBe(other.toLowerCase());
  await second.evaluate(() => localStorage.removeItem('settings'));

  // Closing the window closes its session.
  await second.close({ runBeforeUnload: true });
  await expect.poll(() => page.evaluate(() => window.__arimaa!.api.listSessions())).not.toContain(id);
  expect(await page.evaluate(() => window.__arimaa!.state()!.ply)).toBe(1);
});
