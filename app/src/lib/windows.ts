// Windows (docs/WINDOWS.md): the main window is the lobby, with no
// session; each game window shows its own session, named in its URL
// (`?session=<id>`). In Tauri a game window's label is `game-<session>`,
// and the backend ends the session when the window is destroyed
// (`Backend::window_closed`) and hides the lobby instead of closing it
// while game windows are open (`lib.rs`). In the browser preview a window
// is a tab, named so the others can find it, and a game window's tab
// closes its session itself on `pagehide`.
import { type Api, api, apiFor, isGameWindow, session } from './api';
import type { MatchSpec } from './bindings/MatchSpec';
import type { SessionId } from './bindings/SessionId';

export const inTauri = '__TAURI_INTERNALS__' in window;

/** The browser preview's tab names, so a tab can bring another forward. */
const LOBBY_TAB = 'howdah-lobby';
const gameTab = (id: SessionId) => `howdah-game-${id}`;

/** What a new game window takes over from the window that opened it. */
export interface HandOff {
  /** The match it starts with, for a rematch. */
  spec: MatchSpec | null;
}

const handOffKey = (id: SessionId) => `handoff.${id}`;

/** Opens a game window on a new session. `prepare` sets the session up
 * first (a match, a record, an arimaa.com game), so a failure leaves no
 * window behind; its error is thrown. */
export async function openGameWindow(prepare?: (a: Api) => Promise<void>, handOff?: HandOff): Promise<SessionId> {
  const id = await api.openSession();
  try {
    await prepare?.(apiFor(id));
    if (handOff) {
      try {
        localStorage.setItem(handOffKey(id), JSON.stringify(handOff));
      } catch {
        /* the window just won't know */
      }
    }
    const url = `index.html?session=${id}`;
    if (inTauri) {
      const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
      const w = new WebviewWindow(`game-${id}`, {
        url,
        title: 'Howdah',
        width: 1180,
        height: 800,
        minWidth: 640,
        minHeight: 480,
      });
      await new Promise<void>((resolve, reject) => {
        void w.once('tauri://created', () => resolve());
        void w.once<string>('tauri://error', (e) => reject(new Error(`Couldn't open a window: ${e.payload}`)));
      });
    } else if (!window.open(url, gameTab(id))) {
      throw new Error("Couldn't open a window (the browser blocked it)");
    }
  } catch (e) {
    await api.closeSession(id).catch(() => {});
    throw e;
  }
  return id;
}

/** What the window that opened this one handed over, once. */
export function takeHandOff(): HandOff | null {
  if (session == null) return null;
  try {
    const raw = localStorage.getItem(handOffKey(session));
    localStorage.removeItem(handOffKey(session));
    return raw ? (JSON.parse(raw) as HandOff) : null;
  } catch {
    return null;
  }
}

/** Shows and focuses a window by its Tauri label. */
async function bringForward(label: string): Promise<boolean> {
  const { Window } = await import('@tauri-apps/api/window');
  const w = await Window.getByLabel(label);
  if (!w) return false;
  await w.show();
  await w.unminimize();
  await w.setFocus();
  return true;
}

/** In the browser preview, the tab named `name`, opening `url` there if
 * there's no such tab yet. Browsers may not switch to it. */
function browserTab(name: string, url: string) {
  const w = window.open('', name);
  if (w && w.location.href === 'about:blank') w.location.href = url;
  w?.focus();
}

/** Shows the lobby (hidden, in Tauri, when it was closed). */
export async function showLobby() {
  if (inTauri) await bringForward('main');
  else browserTab(LOBBY_TAB, 'index.html');
}

/** Brings game window `id` forward. */
export async function focusGameWindow(id: SessionId) {
  if (inTauri) await bringForward(`game-${id}`);
  else browserTab(gameTab(id), `index.html?session=${id}`);
}

/** Names the browser preview's tab, so other tabs can bring it forward. */
export function nameTab() {
  if (!inTauri) window.name = session == null ? LOBBY_TAB : gameTab(session);
}

/** Asks before the window closes while `warning()` gives a reason to (a
 * game or match that closing would end): `ask(reason)` shows it and
 * resolves to whether to close anyway. In Tauri the backend then ends
 * the session as the window goes; the browser preview can only show its
 * own generic prompt. */
export async function guardClose(warning: () => string | null, ask: (reason: string) => Promise<boolean>) {
  if (!inTauri) {
    window.addEventListener('beforeunload', (e) => {
      if (warning()) e.preventDefault();
    });
    return;
  }
  const { getCurrentWindow } = await import('@tauri-apps/api/window');
  const win = getCurrentWindow();
  let asking = false;
  await win.onCloseRequested(async (e) => {
    const reason = warning();
    if (!reason) return;
    e.preventDefault();
    if (asking) return;
    asking = true;
    try {
      if (await ask(reason)) await win.destroy();
    } finally {
      asking = false;
    }
  });
}

/** In the browser preview, a game window's tab ends its session when it
 * goes away. Tauri's windows leave that to the backend. */
export async function closeSessionWithPage() {
  if (!import.meta.env.DEV || inTauri || !isGameWindow) return;
  const { closeSessionOnUnload } = await import('./devBridge');
  window.addEventListener('pagehide', () => {
    if (session != null) closeSessionOnUnload(session);
  });
}
