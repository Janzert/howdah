// The position editor (docs/UI-SURVEY.md, section 6), opened from the lobby
// and from a game window, each in a window of its own.
import { expect, test, type BrowserContext, type Page } from '@playwright/test';
import { bridge, closeAllSessions } from './helpers';

async function opens(context: BrowserContext, click: () => Promise<void>): Promise<Page> {
  const [win] = await Promise.all([context.waitForEvent('page'), click()]);
  await win.waitForFunction(() => window.__arimaa?.state() != null);
  return win;
}

const editor = (win: Page) => win.evaluate(() => window.__arimaa!.editor());
const board = (win: Page) => win.getByRole('application', { name: 'Arimaa board' });

/** Picks a palette piece as the click tool. */
async function pick(win: Page, name: string) {
  await win.getByRole('button', { name: new RegExp(`^${name}, `) }).click();
}

test('the lobby opens the editor; pieces go on and off, and Analyse starts from the position', async ({
  page,
  context,
}) => {
  await closeAllSessions(page);
  await page.goto('/');
  const win = await opens(context, () => page.getByRole('button', { name: 'Position editor' }).click());
  await expect(win.getByRole('main', { name: 'Position editor' })).toBeVisible();
  await expect(board(win).getByRole('img')).toHaveCount(32);
  expect((await editor(win)).problems).toEqual([]);

  await win.getByRole('button', { name: 'Clear board' }).click();
  await expect.poll(async () => (await editor(win)).problems).toEqual(['Gold has no rabbits', 'Silver has no rabbits']);
  await expect(win.getByRole('button', { name: 'Analyse' })).toBeDisabled();

  // Place with the palette as a click tool; a second click on the same piece takes it off.
  await pick(win, 'gold rabbit');
  await win.evaluate(async () => {
    const a = window.__arimaa!;
    await a.click('a2');
    await a.click('b2');
    await a.click('b2');
  });
  await pick(win, 'gold dog');
  await win.evaluate(() => window.__arimaa!.click('c3'));
  await pick(win, 'silver rabbit');
  await win.evaluate(() => window.__arimaa!.click('h7'));
  await expect.poll(async () => (await editor(win)).problems).toEqual(['Gold dog alone on trap c3']);
  await expect(win.locator('[data-marked="c3"]')).toHaveCount(1);
  // A right click takes the dog off, whatever the tool.
  await win.evaluate(() => window.__arimaa!.rightClick('c3'));
  await expect.poll(async () => (await editor(win)).problems).toEqual([]);

  // Typed placements, and silver to move at move 7.
  await win.getByLabel('Type pieces').fill('Ed4 ee5 s');
  await win.getByLabel('Type pieces').press('Enter');
  await expect.poll(async () => (await editor(win)).label).toBe('2s');
  await win.getByLabel('Move', { exact: true }).fill('7');
  await win.getByLabel('Move', { exact: true }).dispatchEvent('change');
  await expect.poll(async () => (await editor(win)).label).toBe('7s');
  const short = (await editor(win)).short;
  expect(short.startsWith('s [')).toBe(true);
  await expect(board(win).getByRole('img', { name: 'silver elephant e5' })).toBeVisible();

  await win.getByRole('button', { name: 'Analyse' }).click();
  await expect(win.getByRole('main', { name: 'Position editor' })).toBeHidden();
  await expect(win.getByText('Silver to move')).toBeVisible();
  await win.evaluate(async () => {
    const a = window.__arimaa!;
    await a.drag('e5', 'e6');
    await a.api.commitTurn(false);
    await a.idle();
  });
  const moves = await win.evaluate(() => window.__arimaa!.state()!.moves.map((m) => `${m.label} ${m.notation}`));
  expect(moves).toEqual(['7s ee5n']);
  const record = await win.evaluate(() => window.__arimaa!.api.exportGame());
  expect(record).toContain(`[Position "${short}"]`);
});

test('Edit position opens the shown position in a new window, and Play starts a match from it', async ({
  page,
  context,
}) => {
  await closeAllSessions(page);
  const engines = await bridge<{ id: string }[]>(page, 'list_engines');
  test.skip(engines.length === 0, 'no test engine (cargo build -p howdah-aei --bin aei-test-engine)');
  const id = await bridge<number>(page, 'open_session');
  await page.goto(`/?session=${id}`);
  await page.waitForFunction(() => window.__arimaa?.state() != null);
  await page.evaluate(async () => {
    const a = window.__arimaa!;
    await a.api.commitSetup();
    await a.api.commitSetup();
    await a.drag('d2', 'd3');
    await a.api.commitTurn(false);
    await a.idle();
  });

  const win = await opens(context, () => page.keyboard.press('e'));
  await expect.poll(async () => (await editor(win)).label).toBe('2s');
  await expect(board(win).getByRole('img', { name: 'gold camel d3' })).toBeVisible();

  await win.getByRole('button', { name: 'Play…' }).click();
  const dialog = win.getByRole('dialog', { name: 'New game' });
  await expect(dialog).toContainText('From the edited position: Silver to move, move 2');
  await dialog.locator('#ng-gold').selectOption(engines[0].id);
  await dialog.locator('#ng-silver').selectOption('human');
  await dialog.getByRole('button', { name: 'Start' }).click();
  await expect(win.getByRole('main', { name: 'Position editor' })).toBeHidden();
  // Silver (the human) moves first; then the engine answers as gold.
  await win.evaluate(async () => {
    const a = window.__arimaa!;
    await a.idle();
    await a.drag('d7', 'd6');
    await a.api.commitTurn(false);
  });
  await expect.poll(() => win.evaluate(() => window.__arimaa!.state()!.moves.length)).toBe(2);
  const labels = await win.evaluate(() => window.__arimaa!.state()!.moves.map((m) => m.label));
  expect(labels).toEqual(['2s', '3g']);
  // The game window it came from still has its own game.
  expect(await page.evaluate(() => window.__arimaa!.state()!.moves.length)).toBe(3);
});
