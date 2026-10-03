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
    // The bridge is shared between tests; analysis stays on across games.
    await a.api.setAnalysis(null);
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

// Gold to move after 3s: the gold elephant on g4 stands over the silver horse on g3.
const SAMPLE_TO_3S = [
  '1g Ha2 Db2 Cc2 Md2 Ee2 Cf2 Dg2 Hh2 Ra1 Rb1 Rc1 Rd1 Re1 Rf1 Rg1 Rh1',
  '1s ha7 db7 cc7 md7 ee7 cf7 dg7 hh7 ra8 rb8 rc8 rd8 re8 rf8 rg8 rh8',
  '2g Ee2n Ee3n Ee4n Ee5e',
  '2s hh7s hh6s hh5w',
  '3g hg5s Ef5e hg4s Eg5s',
  '3s rh8s rh7s rh6s',
];

async function load(page: Page, lines: string[]) {
  await page.evaluate(async (r) => {
    await window.__arimaa!.api.loadGame(r);
    await window.__arimaa!.idle();
  }, lines.join('\n'));
}

test('last move arrows and captured pieces', async ({ page }) => {
  await freshGame(page);
  // The elephant pushes the horse onto the f3 trap, where it's captured.
  await load(page, [...SAMPLE_TO_3S, '4g hg3w hf3x Eg4s Eg3n']);
  // The push: a dashed trail for the horse, a solid one for the elephant.
  await expect(page.locator('.last-move .trail')).toHaveCount(2);
  await expect(page.locator('.last-move .trail.displaced')).toHaveCount(1);
  await expect(page.locator('.last-move .ghost')).toHaveCount(1);
  await expect(page.getByRole('img', { name: 'captured: silver horse' })).toBeVisible();

  // Taking a step hides the opponent's last move.
  await page.evaluate(async () => {
    await window.__arimaa!.api.gotoPly(6);
    await window.__arimaa!.idle();
  });
  await expect(page.getByRole('img', { name: /^captured/ })).toHaveCount(0);
  await page.evaluate(() => window.__arimaa!.drag('a2', 'a3'));
  await expect(page.locator('.last-move')).toHaveCount(0);
});

test('coordinates setting changes the board labels and persists', async ({ page }) => {
  await freshGame(page);
  await page.evaluate(() => localStorage.removeItem('settings'));
  await page.reload();
  const labels = page.locator('svg[aria-label="Arimaa board"] .coord');
  await expect(labels).toHaveText(['c3', 'f3', 'c6', 'f6']); // the default: traps

  await page.getByRole('button', { name: 'Settings' }).click();
  const dialog = page.getByRole('dialog', { name: 'Settings' });
  await dialog.getByLabel('Files and ranks').check();
  await expect(labels).toHaveCount(16);
  await dialog.getByLabel('None').check();
  await expect(labels).toHaveCount(0);
  await dialog.getByRole('button', { name: 'Done' }).click();
  await expect(dialog).toBeHidden();

  await page.reload();
  await page.waitForFunction(() => window.__arimaa?.state() != null);
  await expect(labels).toHaveCount(0);
  await page.evaluate(() => localStorage.removeItem('settings'));
});

test('hover arrows show legal steps and a click takes one', async ({ page }) => {
  await page.goto('/');
  await page.evaluate(() => localStorage.setItem('settings', JSON.stringify({ hoverInput: 'arrows' })));
  await freshGame(page);
  await load(page, SAMPLE_TO_3S);
  const hover = (sq: string) =>
    page.evaluate(async (s) => {
      await window.__arimaa!.hover(s);
      return window.__arimaa!.hoverTargets().sort();
    }, sq);
  expect(await hover('g4')).toEqual(['f4', 'g5', 'h4']);
  // An enemy piece shows the pushes that would start from it.
  expect(await hover('g3')).toEqual(['f3', 'h3']);
  await expect(page.locator('.hover-arrow.enemy')).toHaveCount(2);
  // The arrows stay while the pointer is on one, and a click takes the step.
  expect(await hover('f3')).toEqual(['f3', 'h3']);
  await page.evaluate(() => window.__arimaa!.click('f3'));
  expect(await page.evaluate(() => window.__arimaa!.state()!.turn!.steps.map((s) => s.notation))).toEqual([
    'hg3w hf3x',
  ]);
  // Only the push finish is left for the elephant.
  expect(await hover('g4')).toEqual(['g3']);
  await page.evaluate(() => localStorage.removeItem('settings'));
});

test('forward at the latest move replays it', async ({ page }) => {
  await freshGame(page);
  await load(page, SAMPLE_TO_3S);
  const h8 = () => page.evaluate(() => window.__arimaa!.board().split('\n')[1].slice(-3, -2));
  const h5 = () => page.evaluate(() => window.__arimaa!.board().split('\n')[4].slice(-3, -2));
  expect(await h8()).toBe('.');
  await page.getByRole('button', { name: 'Forward' }).click();
  // 3s walked the rabbit h8-h5: it jumps back to h8, then walks again.
  await expect.poll(h5, { intervals: [20] }).toBe('.');
  await page.evaluate(() => window.__arimaa!.idle());
  expect(await h5()).toBe('r');
  expect(await page.evaluate(() => window.__arimaa!.state()!.ply)).toBe(6);
});

test('step mode offers the step toward the pointer and a click takes it', async ({ page }) => {
  await page.goto('/');
  await page.evaluate(() => localStorage.setItem('settings', JSON.stringify({ hoverInput: 'step' })));
  await freshGame(page);
  await load(page, SAMPLE_TO_3S);
  const offer = (sq: string, toward: string) =>
    page.evaluate(
      async ([s, t]) => {
        await window.__arimaa!.hover(s, t);
        return window.__arimaa!.hoverTargets();
      },
      [sq, toward],
    );
  expect(await offer('g4', 'g5')).toEqual(['g5']);
  expect(await offer('g4', 'g3')).toEqual([]); // occupied by the horse: not a step
  // The horse, leaning toward the f3 trap: the push starting there.
  expect(await offer('g3', 'f3')).toEqual(['f3']);
  await expect(page.locator('.hover-arrow.enemy')).toHaveCount(1);
  // Over an empty square, the neighbour stepping in: the elephant into f4.
  expect(await offer('f4', 'g4')).toEqual(['f4']);

  // A click on the elephant takes its step; dragging still works.
  await page.evaluate(() => window.__arimaa!.click('g4', 'g5'));
  const steps = () => page.evaluate(() => window.__arimaa!.state()!.turn!.steps.map((s) => s.notation));
  expect(await steps()).toEqual(['Eg4n']);
  await page.evaluate(() => window.__arimaa!.drag('g5', 'g6'));
  expect(await steps()).toEqual(['Eg4n', 'Eg5n']);
  await page.evaluate(() => localStorage.removeItem('settings'));
});

test('a game ending in play shows the result once', async ({ page }) => {
  await freshGame(page);
  await page.evaluate(() =>
    window.__arimaa!.api.startMatch({
      gold: { kind: 'human' },
      silver: { kind: 'human' },
      goldTimeControl: '1s/1s',
      silverTimeControl: null,
    }),
  );
  const dialog = page.getByRole('dialog', { name: 'Silver wins' });
  await expect(dialog).toBeVisible({ timeout: 5000 });
  await expect(dialog.getByText('Gold ran out of time.')).toBeVisible();
  // Started without the New game dialog, so there's no match to swap.
  await expect(dialog.getByRole('button', { name: 'Swap sides' })).toHaveCount(0);
  await dialog.getByRole('button', { name: 'Review game' }).click();
  await expect(dialog).toHaveCount(0);

  // Returning to the end of the finished game doesn't announce it again.
  await page.keyboard.press('Home');
  await page.keyboard.press('End');
  await page.evaluate(() => window.__arimaa!.idle());
  await expect(page.getByRole('dialog')).toHaveCount(0);
});

test('? opens and closes the keyboard help; k and j step through moves', async ({ page }) => {
  await freshGame(page);
  await load(page, SAMPLE_TO_3S);
  const help = page.getByRole('dialog', { name: 'Keyboard and mouse' });
  await page.keyboard.press('?');
  await expect(help).toBeVisible();
  await expect(help.getByText('Flip the board')).toBeVisible();
  // Keys meant for the board do nothing while the help is open.
  const ply = () => page.evaluate(() => window.__arimaa!.state()!.ply);
  const before = await ply();
  await page.keyboard.press('k');
  expect(await ply()).toBe(before);
  await page.keyboard.press('?');
  await expect(help).toHaveCount(0);
  // Focus returns to the page once the dialog is gone.
  await expect.poll(() => page.evaluate(() => document.activeElement?.tagName)).toBe('BODY');

  await page.keyboard.press('k');
  await expect.poll(ply).toBe(before - 1);
  await page.keyboard.press('j');
  await expect.poll(ply).toBe(before);
});

test('the move list shows variations; a click follows one, and the menu deletes it', async ({ page }) => {
  await freshGame(page);
  await load(page, [...SAMPLE_TO_3S.slice(0, 2), '2g Ee2n Ee3n', '(', '2g Ha2n', '2s hh7s', ')', '2s ee7s']);
  const list = page.locator('.moves');
  const variation = list.getByRole('button', { name: /2g\s+Ha2n/ });
  await expect(variation).toBeVisible();
  await variation.click();
  await expect.poll(() => page.evaluate(() => window.__arimaa!.state()!.moves.at(-1)?.notation)).toBe('hh7s');
  await expect(board(page).getByRole('img', { name: 'gold horse a3' })).toBeVisible();

  await variation.click({ button: 'right' });
  await page.getByRole('menuitem', { name: 'Delete from here' }).click();
  await expect(variation).toHaveCount(0);
  expect(await page.evaluate(() => window.__arimaa!.state()!.ply)).toBe(2);
});

test('arrow keys switch lines, the comment box annotates, and a variation folds', async ({ page }) => {
  await freshGame(page);
  await load(page, [...SAMPLE_TO_3S.slice(0, 2), '2g Ee2n Ee3n', '(', '2g Ha2n', '2s hh7s', ')', '2s ee7s']);
  const cursor = () =>
    page.evaluate(() => {
      const s = window.__arimaa!.state()!;
      return s.tree.find((m) => m.id === s.cursor)?.notation;
    });
  await page.keyboard.press('Shift+ArrowLeft');
  await expect.poll(cursor).toBe('Ee2n Ee3n');
  await page.keyboard.press('ArrowDown');
  await expect.poll(cursor).toBe('Ha2n');
  await page.keyboard.press('ArrowUp');
  await expect.poll(cursor).toBe('Ee2n Ee3n');
  await page.keyboard.press('ArrowDown');

  await page.getByRole('button', { name: 'Interesting move' }).click();
  const comment = page.getByRole('textbox', { name: /Comment on 2g Ha2n/ });
  await comment.fill('A quieter start.');
  await comment.press('Control+Enter');
  const list = page.locator('.moves');
  await expect(list.getByText('A quieter start.')).toBeVisible();
  await expect(list.getByRole('button', { name: /2g\s+Ha2n\s*!\?/ })).toBeVisible();

  // Folded variations stay open while the board shows a move inside them.
  await list.getByRole('button', { name: /2s\s+ee7s/ }).click();
  await page.getByRole('button', { name: 'Fold the variation from 2g' }).click();
  await expect(list.getByText('+1')).toBeVisible();
  await expect(list.getByRole('button', { name: /2s\s+hh7s/ })).toHaveCount(0);
});

test('l turns analysis on; a click on its line adds it, and l turns it off', async ({ page }) => {
  await freshGame(page);
  await load(page, SAMPLE_TO_3S);
  await page.keyboard.press('l');
  const panel = page.getByRole('region', { name: 'Analysis' });
  await expect(panel).toBeVisible();
  // The bundled random mover answers at once; its move is the line.
  await panel.getByRole('combobox', { name: 'Analysis engine' }).selectOption({ label: 'Random mover (test engine)' });
  await expect(page.getByRole('meter', { name: 'Evaluation for gold' })).toBeVisible();
  const chip = panel.getByRole('button', { name: /^4g\s/ });
  await expect(chip).toBeVisible();
  const ply = () => page.evaluate(() => window.__arimaa!.state()!.ply);
  const before = await ply();
  await chip.click();
  await expect.poll(ply).toBe(before + 1);
  // Analysis follows the board to the new node.
  await expect(panel.getByText(/Done: 4s/)).toBeVisible();

  await page.locator('body').click({ position: { x: 1, y: 1 } });
  await page.keyboard.press('l');
  await expect(panel).toHaveCount(0);
  expect(await page.evaluate(() => window.__arimaa!.state()!.analysisEngine)).toBeNull();
});
