// The lobby and game windows (docs/WINDOWS.md): in the browser preview the
// lobby is the page without `?session=`, and each game window a tab opened
// with `window.open`, on its own session.
import { expect, test, type BrowserContext, type Page } from '@playwright/test';
import { boardsInWindows, bridge, closeAllSessions } from './helpers';

/** Opens the lobby with no boards. */
async function lobby(page: Page) {
  await closeAllSessions(page);
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Play and analyse' })).toBeVisible();
}

/** Clicks `button` in the lobby and returns the game window it opens. */
async function opens(context: BrowserContext, click: () => Promise<void>): Promise<Page> {
  const [game] = await Promise.all([context.waitForEvent('page'), click()]);
  await game.waitForFunction(() => window.__arimaa?.state() != null);
  return game;
}

const sessionOf = (game: Page) => Number(new URL(game.url()).searchParams.get('session'));
const openWindows = (page: Page) => page.getByRole('list', { name: 'Boards', exact: true }).getByRole('listitem');
const sidebar = (page: Page) => page.getByRole('navigation', { name: 'Lobby' });

test('the lobby opens game windows, lists them, and closing one ends its session', async ({ page, context }) => {
  await boardsInWindows(page);
  await lobby(page);
  await expect(page.getByText('No boards are open')).toBeVisible();

  // A game against the test engine: the lobby lists it as waiting on the user.
  const engines = await bridge<{ id: string }[]>(page, 'list_engines');
  test.skip(engines.length === 0, 'no test engine (cargo build -p howdah-aei --bin aei-test-engine)');
  await page.getByRole('button', { name: 'New game' }).click();
  const dialog = page.getByRole('dialog', { name: 'New game' });
  await dialog.locator('#ng-silver').selectOption(engines[0].id);
  const first = await opens(context, () => dialog.getByRole('button', { name: 'Start' }).click());
  const firstId = sessionOf(first);
  await expect(openWindows(page)).toHaveCount(1);
  await expect(openWindows(page).first()).toContainText('Your move');
  await expect(openWindows(page).first()).toContainText('Against an engine');

  // A second, empty window from the game window's New window.
  const second = await opens(context, () => first.getByRole('button', { name: 'New board' }).click());
  const secondId = sessionOf(second);
  expect(secondId).not.toBe(firstId);
  await expect(openWindows(page)).toHaveCount(2);

  // Moves in one window don't reach the other.
  await second.evaluate(async () => {
    const a = window.__arimaa!;
    await a.api.commitSetup();
    await a.api.commitSetup();
    await a.idle();
  });
  await expect(second.getByText('Gold to move')).toBeVisible();
  expect(await first.evaluate(() => window.__arimaa!.state()!.ply)).toBe(0);
  // The lobby follows the game: gold's setup confirmed, the engine replies.
  await first.evaluate(async () => {
    await window.__arimaa!.api.commitSetup();
  });
  await expect.poll(() => first.evaluate(() => window.__arimaa!.state()!.moves.length)).toBe(2);
  await expect(openWindows(page).filter({ hasText: 'Your move' })).toHaveCount(1);

  // A setting changed in the lobby applies in the game windows.
  const theme = () => second.evaluate(() => document.documentElement.dataset.theme);
  const other = (await theme()) === 'dark' ? 'Light' : 'Dark';
  await sidebar(page).getByRole('button', { name: 'Settings' }).click();
  await page.getByLabel(other, { exact: true }).check();
  await expect.poll(theme).toBe(other.toLowerCase());
  await page.evaluate(() => localStorage.removeItem('settings'));
  await sidebar(page).getByRole('button', { name: 'Home' }).click();

  // Closing a window ends its session, and the lobby drops it.
  await second.close({ runBeforeUnload: true });
  await expect.poll(() => bridge<number[]>(page, 'list_sessions')).toEqual([firstId]);
  await expect(openWindows(page)).toHaveCount(1);
});

test('the lobby opens a record in a window', async ({ page, context }) => {
  await boardsInWindows(page);
  await lobby(page);
  await page.getByRole('button', { name: 'Open record' }).click();
  const dialog = page.getByRole('dialog', { name: 'Open a record' });
  await dialog.getByRole('textbox').fill('1g Ra1 Rb1 Rc1 Rd1 Re1 Rf1 Rg1 Rh1 Ha2 Db2 Cc2 Md2 Ee2 Cf2 Dg2 Hh2\n');
  const game = await opens(context, () => dialog.getByRole('button', { name: 'Load' }).click());
  await expect(dialog).toBeHidden();
  expect(await game.evaluate(() => window.__arimaa!.state()!.moves.length)).toBe(1);
  await expect(openWindows(page)).toHaveCount(1);
});

test('the sidebar switches panels, lists boards, and folds to boards', async ({ page, context }) => {
  await boardsInWindows(page);
  await lobby(page);
  await sidebar(page).getByRole('button', { name: 'Engine tests' }).click();
  await expect(page.getByRole('heading', { name: 'Engine tests', level: 1 })).toBeVisible();
  await sidebar(page).getByRole('button', { name: 'Home' }).click();
  const game = await opens(context, () => page.getByRole('button', { name: 'Analysis board' }).click());
  const listed = sidebar(page).getByRole('list', { name: 'Open boards' }).getByRole('button', { name: /^Show / });
  await expect(listed).toHaveCount(1);

  // Folded, the sidebar keeps its buttons (by name) and the board, and
  // stays folded after a reload, on the panel last shown.
  await sidebar(page).getByRole('button', { name: 'Collapse the sidebar' }).click();
  await sidebar(page).getByRole('button', { name: 'Help' }).click();
  await page.reload();
  await expect(sidebar(page).getByRole('button', { name: 'Expand the sidebar' })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Help', level: 1 })).toBeVisible();
  await expect(listed).toHaveCount(1);
  await sidebar(page).getByRole('button', { name: 'Expand the sidebar' }).click();
  await sidebar(page).getByRole('button', { name: 'Home' }).click();
  await game.close();
  await expect(listed).toHaveCount(0);
});

test('a board opens as a panel, pops out to a window, docks back, and closes', async ({ page, context }) => {
  await lobby(page);
  await page.getByRole('button', { name: 'Analysis board' }).click();
  const listed = sidebar(page).getByRole('list', { name: 'Open boards' }).getByRole('button', { name: /^Show / });
  await expect(listed).toHaveCount(1);
  const docked = page.frameLocator('iframe.docked');
  await expect(docked.getByRole('application', { name: 'Arimaa board' })).toBeVisible();
  const [id] = await bridge<number[]>(page, 'list_sessions');
  expect(await bridge<number[]>(page, 'docked_sessions')).toEqual([id]);

  // Pop out: the board moves to a window, on the same session.
  const win = await opens(context, () => docked.getByRole('button', { name: 'Pop out' }).click());
  expect(sessionOf(win)).toBe(id);
  await expect(page.locator('iframe.docked')).toHaveCount(0);
  await expect(listed).toHaveCount(1);
  expect(await bridge<number[]>(page, 'docked_sessions')).toEqual([]);

  // Dock: the window closes and the board is a panel again.
  await Promise.all([win.waitForEvent('close'), win.getByRole('button', { name: 'Dock' }).click()]);
  await expect(docked.getByRole('application', { name: 'Arimaa board' })).toBeVisible();
  expect(await bridge<number[]>(page, 'list_sessions')).toEqual([id]);

  // Close from the sidebar ends the session.
  await sidebar(page).getByRole('button', { name: /^Close / }).click();
  await expect(listed).toHaveCount(0);
  await expect.poll(() => bridge<number[]>(page, 'list_sessions')).toEqual([]);
});

test('Alt+arrows and Alt+digits go through the sidebar, from a docked board too', async ({ page }) => {
  await lobby(page);
  const current = () => sidebar(page).locator('[aria-current="page"]').getAttribute('aria-label');
  await page.keyboard.press('Alt+ArrowDown');
  expect(await current()).toMatch(/^arimaa\.com/);
  await expect(page.locator('main h1')).toBeFocused();
  await page.keyboard.press('Alt+1');
  expect(await current()).toBe('Home');
  await page.getByRole('button', { name: 'Analysis board' }).click();
  const docked = page.frameLocator('iframe.docked');
  await expect(docked.getByRole('application', { name: 'Arimaa board' })).toBeVisible();
  await docked.getByRole('list', { name: 'Moves' }).locator('button.current').focus();
  await page.keyboard.press('Alt+ArrowUp');
  expect(await current()).toBe('Engine tests');
});

test("a game window's Lobby button goes back to the lobby tab", async ({ page, context }) => {
  await boardsInWindows(page);
  await lobby(page);
  const game = await opens(context, () => page.getByRole('button', { name: 'Analysis board' }).click());
  await game.getByRole('button', { name: /^Lobby/ }).click();
  // The lobby's tab is found by name, not opened again.
  await game.waitForTimeout(300);
  expect(context.pages()).toHaveLength(2);
  await expect(page.getByRole('heading', { name: 'Play and analyse' })).toBeVisible();
});

test('titles say whose move it is, and Next game goes to the other waiting game', async ({ page, context }) => {
  await boardsInWindows(page);
  await lobby(page);
  const engines = await bridge<{ id: string }[]>(page, 'list_engines');
  test.skip(engines.length === 0, 'no test engine (cargo build -p howdah-aei --bin aei-test-engine)');
  /** A game against the engine, the user setting up first. */
  const engineGame = async () => {
    await page.getByRole('button', { name: 'New game' }).click();
    const dialog = page.getByRole('dialog', { name: 'New game' });
    await dialog.locator('#ng-silver').selectOption(engines[0].id);
    return opens(context, () => dialog.getByRole('button', { name: 'Start' }).click());
  };
  const first = await engineGame();
  const second = await engineGame();
  await expect(first).toHaveTitle(/^Your move · Human - .* – Howdah$/);
  const next = (p: Page) => p.getByRole('button', { name: /^Next game/ });
  await expect(next(first)).toContainText('1');
  await expect(next(second)).toContainText('1');

  // n finds the other window rather than opening one.
  await first.locator('body').press('n');
  await first.waitForTimeout(300);
  expect(context.pages()).toHaveLength(3);

  // After the engine's setup it's the user's move again there.
  await second.evaluate(() => window.__arimaa!.api.commitSetup());
  await expect.poll(() => second.evaluate(() => window.__arimaa!.state()!.moves.length)).toBe(2);
  await expect(second).toHaveTitle(/^Your move/);
  await expect(next(first)).toContainText('1');
});
