// Shared by the e2e tests.
import type { Page } from '@playwright/test';
import type { ArimaaHooks } from '../src/lib/devHooks';

declare global {
  interface Window {
    __arimaa?: ArimaaHooks;
  }
}

/** Calls a dev-bridge command directly. */
export async function bridge<T>(page: Page, cmd: string, args: object = {}): Promise<T> {
  const res = await page.request.post(`/bridge/invoke/${cmd}`, { data: args });
  if (!res.ok()) throw new Error(`${cmd}: ${await res.text()}`);
  const text = await res.text();
  return (text ? JSON.parse(text) : null) as T;
}

/** Closes every session the bridge has open (tests share the bridge, and
 * run one at a time). */
export async function closeAllSessions(page: Page) {
  for (const id of await bridge<number[]>(page, 'list_sessions')) {
    await bridge(page, 'close_session', { session: id }).catch(() => {});
  }
}

/** Opens a game window (a tab, in the browser) on a new session, as the
 * only one, and waits for its game. */
export async function openGamePage(page: Page): Promise<number> {
  await closeAllSessions(page);
  const id = await bridge<number>(page, 'open_session');
  await page.goto(`/?session=${id}`);
  await page.waitForFunction(() => window.__arimaa?.state() != null);
  return id;
}
