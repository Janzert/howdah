// Solving a puzzle in a game window. The puzzle is made up, in the format
// of arimaa.com's puzzle pages, and loaded as text: the tests never talk
// to arimaa.com.
import { expect, test, type Page } from '@playwright/test';
import { openGamePage } from './helpers';

/** Gold to win in two: the rabbit runs up, silver steps aside, goal. */
const PUZZLE =
  'mode=selfPlay\n&movelist=1w Ra5 Ee1%0d1b rh7 eh8%0d2w Ra5n Ra6n%0d2b rh7w rg7w%0d3w Ra7n%0d3b \n' +
  '&chat=Gold to win in two\n\n&title=A made-up puzzle\n&side=w\n&startmove=2w\n';

const panel = (page: Page) => page.getByRole('region', { name: 'Puzzle' });

async function play(page: Page, steps: [string, string][]) {
  await page.evaluate(async (steps) => {
    const a = window.__arimaa!;
    const sq = (n: string) => (Number(n[1]) - 1) * 8 + 'abcdefgh'.indexOf(n[0]);
    for (const [f, t] of steps) await a.api.tryStep(sq(f), sq(t));
    await a.api.commitTurn(false);
    await a.idle();
  }, steps);
}

test('a puzzle checks moves, plays the reply, and says when it is solved', async ({ page }) => {
  await openGamePage(page);
  await page.evaluate((text) => window.__arimaa!.api.loadGame(text), PUZZLE);
  await expect(panel(page)).toContainText('A made-up puzzle');
  await expect(panel(page)).toContainText('Gold to win in two');
  await expect(panel(page)).toContainText('Gold to play: 2 moves to go');
  await expect(page).toHaveTitle(/^Puzzle: A made-up puzzle/);
  await expect(panel(page).getByRole('button', { name: 'Hint' })).toHaveCount(0);

  await play(page, [['a5', 'b5']]);
  await expect(panel(page)).toContainText('Not the solution: 2g Ra5e');
  await panel(page).getByRole('button', { name: 'Try again' }).click();
  await expect(panel(page)).toContainText('Gold to play: 2 moves to go');

  await play(page, [
    ['a5', 'a6'],
    ['a6', 'a7'],
  ]);
  await expect(panel(page)).toContainText('Gold to play');
  const labels = await page.evaluate(() => window.__arimaa!.state()!.moves.map((m) => `${m.label} ${m.notation}`));
  expect(labels).toEqual(['2g Ra5n Ra6n', '2s rh7w rg7w']);

  await play(page, [['a7', 'a8']]);
  await expect(panel(page)).toContainText('Solved!');
  // No game-end dialog for a puzzle.
  await expect(page.getByRole('dialog')).toHaveCount(0);
});

test('Show answer puts the solution in the move list', async ({ page }) => {
  await openGamePage(page);
  await page.evaluate((text) => window.__arimaa!.api.loadGame(text), PUZZLE);
  await panel(page).getByRole('button', { name: 'Show answer' }).click();
  await expect(panel(page)).toContainText('The answer is in the move list');
  const moves = await page.evaluate(() => window.__arimaa!.state()!.tree.map((m) => m.notation));
  expect(moves).toEqual(['Ra5n Ra6n', 'rh7w rg7w', 'Ra7n']);
});
