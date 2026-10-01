// Smoke tests through the UI, using the board's accessible names and the dev
// hook (`window.__arimaa`, see src/lib/devHooks.ts).
import { expect, test, type Page } from '@playwright/test';
import type { ArimaaHooks } from '../src/lib/devHooks';

declare global {
  interface Window {
    __arimaa?: ArimaaHooks;
  }
}

/** Opens the app on a fresh free-play game. */
async function freshGame(page: Page) {
  await page.goto('/');
  await page.waitForFunction(() => window.__arimaa?.state() != null);
  await page.evaluate(async () => {
    const a = window.__arimaa!;
    await a.api.newGame();
    await a.idle();
  });
}

const board = (page: Page) => page.getByRole('application', { name: 'Arimaa board' });

test('starts in gold setup with the gold pieces labelled', async ({ page }) => {
  await freshGame(page);
  await expect(page.getByText('Gold setup')).toBeVisible();
  // Silver's pieces appear once gold's setup is confirmed.
  await expect(board(page).getByRole('img')).toHaveCount(16);
  await expect(board(page).getByRole('img', { name: 'gold elephant e2' })).toBeVisible();
});

test('setup swap with the real mouse, then a dragged route', async ({ page }) => {
  await freshGame(page);
  // A real mouse drag: swap the a2 horse with the h1 rabbit.
  const from = await page.evaluate(() => window.__arimaa!.squareCenter('a2'));
  const to = await page.evaluate(() => window.__arimaa!.squareCenter('h1'));
  await page.mouse.move(from.x, from.y);
  await page.mouse.down();
  await page.mouse.move(to.x, to.y, { steps: 12 });
  await page.mouse.up();
  await expect(board(page).getByRole('img', { name: 'gold horse h1' })).toBeVisible();

  await page.getByRole('button', { name: 'Confirm setup' }).click();
  await page.getByRole('button', { name: 'Confirm setup' }).click();
  await expect(page.getByText('Gold to move')).toBeVisible();

  // Drag the camel three squares up; the route fills in the steps.
  await page.evaluate(() => window.__arimaa!.drag('d2', 'd5', ['d3', 'd4']));
  await expect(board(page).getByRole('img', { name: 'gold camel d5' })).toBeVisible();
  const steps = await page.evaluate(() => window.__arimaa!.state()!.turn!.steps.map((s) => s.notation));
  expect(steps).toEqual(['Md2n', 'Md3n', 'Md4n']);

  await page.getByRole('button', { name: 'Commit' }).click();
  await expect(page.getByText('Silver to move')).toBeVisible();
});

test('an unreachable drop shows an error and puts the piece back', async ({ page }) => {
  await freshGame(page);
  await page.evaluate(async () => {
    const a = window.__arimaa!;
    await a.api.commitSetup();
    await a.api.commitSetup();
    await a.idle();
  });
  // The silver elephant stands on e7.
  await page.evaluate(() => window.__arimaa!.drag('e2', 'e7'));
  await expect.poll(() => page.evaluate(() => window.__arimaa!.message())).not.toBeNull();
  await expect(board(page).getByRole('img', { name: 'gold elephant e2' })).toBeVisible();
});

test('engine vs engine match plays moves', async ({ page }) => {
  await freshGame(page);
  const engines = await page.evaluate(() => window.__arimaa!.api.listEngines());
  test.skip(engines.length === 0, 'no test engine (cargo build -p arimaa-aei --bin aei-test-engine)');
  const engineId = engines[0].id;
  await page.evaluate(
    (id) =>
      window.__arimaa!.api.startMatch({
        gold: { kind: 'engine', engineId: id },
        silver: { kind: 'engine', engineId: id },
        goldTimeControl: '2s/10s',
        silverTimeControl: '2s/10s',
      }),
    engineId,
  );
  await expect.poll(() => page.evaluate(() => window.__arimaa!.state()!.moves.length), { timeout: 15_000 })
    .toBeGreaterThanOrEqual(6);
  await page.evaluate(() => window.__arimaa!.api.endMatch());
});
