// Windows and boards (docs/WINDOWS.md): the main window is the lobby,
// with no session; each board shows its own session, named in its URL
// (`?session=<id>`), either as a panel docked in the lobby (an iframe,
// `&docked`; lib/frames.ts) or in a game window of its own. The backend
// knows which sessions are docked (`set_session_docked`). In Tauri a game
// window's label is `game-<session>`, and the backend ends the session
// when the window is destroyed unless it's docked
// (`Backend::window_closed`), and hides the lobby instead of closing it
// while game windows are open (`lib.rs`). In the browser preview a window
// is a tab, named so the others can find it, and a game window's tab
// closes its session itself on `pagehide`.
import { type Api, api, apiFor, errorMessage, isDocked, isGameWindow, session } from './api';
import type { MatchSpec } from './bindings/MatchSpec';
import type { PositionSpec } from './bindings/PositionSpec';
import type { SessionId } from './bindings/SessionId';
import { clearBoardLoading, setBoardLoading } from './boardLoading';
import { askLobby, onCloseRequest } from './frames';
import { settings } from './settings.svelte';

export const inTauri = '__TAURI_INTERNALS__' in window;

/** The browser preview's tab names, so a tab can bring another forward. */
const LOBBY_TAB = 'howdah-lobby';
const gameTab = (id: SessionId) => `howdah-game-${id}`;

/** What a new game window takes over from the window that opened it. */
export interface HandOff {
  /** The match it starts with, for a rematch. */
  spec: MatchSpec | null;
  /** Opens the position editor on this position. */
  editor?: PositionSpec;
  /** Follows the engine test whose game it opens on: when a game ends,
   * it moves on to the test's newest game without a window. */
  follow?: boolean;
}

const handOffKey = (id: SessionId) => `handoff.${id}`;

/** Leaves `handOff` for the board on session `id`, to take when it opens. */
export function saveHandOff(id: SessionId, handOff: HandOff) {
  try {
    localStorage.setItem(handOffKey(id), JSON.stringify(handOff));
  } catch {
    /* the board just won't know */
  }
}

/** Opens a board on a new session: docked in the lobby or in a window of
 * its own, as the Settings say (from a game window, always a window; from
 * a docked board, the lobby decides). `prepare` sets the session up first
 * (a match, a record, an arimaa.com game), so a failure leaves no board
 * behind; its error is thrown. With `loading` (what the board waits for,
 * for setups that wait on the network), the board is shown at once with
 * that, and `prepare` runs after; if it fails, the board shows the error
 * (`boardLoading.ts`) and it's thrown too. */
export async function openBoard(
  prepare?: (a: Api) => Promise<void>,
  handOff?: HandOff,
  loading?: string,
): Promise<SessionId> {
  const id = await api.openSession();
  if (loading) {
    try {
      if (handOff) saveHandOff(id, handOff);
      setBoardLoading(id, { text: loading });
      await present(id);
    } catch (e) {
      clearBoardLoading(id);
      await api.closeSession(id).catch(() => {});
      throw e;
    }
    try {
      await prepare?.(apiFor(id));
      clearBoardLoading(id);
    } catch (e) {
      setBoardLoading(id, { text: loading, error: errorMessage(e) });
      throw e;
    }
    return id;
  }
  try {
    await prepare?.(apiFor(id));
    if (handOff) saveHandOff(id, handOff);
    await present(id);
  } catch (e) {
    await api.closeSession(id).catch(() => {});
    throw e;
  }
  return id;
}

/** Shows new board `id`, as `openBoard` says. */
async function present(id: SessionId) {
  if (isDocked) askLobby({ kind: 'show', session: id, fresh: true });
  else if (session == null && !settings.boardsInWindows) await api.setSessionDocked(id, true);
  else await openWindowFor(id);
}

/** Closes this board (its session ends): a docked one through the lobby,
 * a window by closing it. Without asking. */
export async function closeThisBoard() {
  if (session == null) return;
  if (isDocked) {
    askLobby({ kind: 'close' });
  } else if (inTauri) {
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    await getCurrentWindow().destroy();
  } else {
    window.close();
  }
}

/** Opens a game window on session `id`, which is open already (a new
 * board, or one popped out of the lobby). Not from a docked board. */
export async function openWindowFor(id: SessionId) {
  const url = `index.html?session=${id}`;
  if (inTauri) {
    const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
    const size = savedSize();
    const w = new WebviewWindow(`game-${id}`, {
      url,
      title: 'Howdah',
      ...size,
      ...(await cascadedPosition(size)),
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
}

const SIZE_KEY = 'gameWindow.size';

/** The size for a new game window: the last one a game window was
 * resized to (4steps keeps one size for all of them). */
function savedSize(): { width: number; height: number } {
  try {
    const s = JSON.parse(localStorage.getItem(SIZE_KEY) ?? 'null');
    if (typeof s?.width === 'number' && typeof s?.height === 'number') {
      return { width: Math.max(640, s.width), height: Math.max(480, s.height) };
    }
  } catch {
    /* the default */
  }
  return { width: 1180, height: 800 };
}

/** Where a new game window goes: down and right of the newest one, back
 * at the top left of the screen if that would run off it; anywhere the
 * system likes when it's the first. */
async function cascadedPosition(size: { width: number; height: number }): Promise<{ x?: number; y?: number }> {
  const { getAllWebviewWindows } = await import('@tauri-apps/api/webviewWindow');
  const { currentMonitor } = await import('@tauri-apps/api/window');
  const games = (await getAllWebviewWindows()).filter((w) => w.label.startsWith('game-'));
  const newest = games.sort((a, b) => Number(b.label.slice(5)) - Number(a.label.slice(5)))[0];
  if (!newest) return {};
  try {
    const scale = await newest.scaleFactor();
    const at = (await newest.outerPosition()).toLogical(scale);
    let { x, y } = { x: at.x + CASCADE, y: at.y + CASCADE };
    const monitor = await currentMonitor();
    if (monitor) {
      const area = monitor.workArea;
      const left = area.position.x / monitor.scaleFactor;
      const top = area.position.y / monitor.scaleFactor;
      if (x + size.width > left + area.size.width / monitor.scaleFactor) x = left + CASCADE;
      if (y + size.height > top + area.size.height / monitor.scaleFactor) y = top + CASCADE;
    }
    return { x, y };
  } catch {
    return {};
  }
}

/** How far each new game window is moved from the last, in logical px. */
const CASCADE = 32;

/** In a game window, saves its size whenever the user resizes it (not
 * while maximized), for the next game window. Tauri only. */
export async function rememberSize() {
  if (!inTauri || !isGameWindow) return;
  const { getCurrentWindow } = await import('@tauri-apps/api/window');
  const win = getCurrentWindow();
  let timer: ReturnType<typeof setTimeout> | undefined;
  await win.onResized(() => {
    clearTimeout(timer);
    timer = setTimeout(async () => {
      if (await win.isMaximized()) return;
      const size = (await win.innerSize()).toLogical(await win.scaleFactor());
      try {
        localStorage.setItem(SIZE_KEY, JSON.stringify({ width: Math.round(size.width), height: Math.round(size.height) }));
      } catch {
        /* not kept */
      }
    }, 500);
  });
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
  if (isDocked) return;
  if (inTauri) await bringForward('main');
  else browserTab(LOBBY_TAB, 'index.html');
}

/** Shows board `id`: its panel in the lobby (the lobby coming forward),
 * or its window. */
export async function showBoard(id: SessionId) {
  if (isDocked) return askLobby({ kind: 'show', session: id });
  if ((await api.dockedSessions()).includes(id)) {
    await api.showBoard(id);
    await showLobby();
    return;
  }
  await focusGameWindow(id);
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
  if (isDocked) {
    // The lobby's close button for the board.
    onCloseRequest(async () => {
      const reason = warning();
      if (!reason || (await ask(reason))) askLobby({ kind: 'close' });
    });
    return;
  }
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

/** Set while a game window docks its board, so its going doesn't close
 * the session. */
let docking = false;

/** In the browser preview, a game window's tab ends its session when it
 * goes away (unless it docked it). Tauri's windows leave that to the
 * backend. */
export async function closeSessionWithPage() {
  if (!import.meta.env.DEV || inTauri || !isGameWindow || isDocked) return;
  const { closeSessionOnUnload } = await import('./devBridge');
  window.addEventListener('pagehide', () => {
    if (session != null && !docking) closeSessionOnUnload(session);
  });
}

/** In a game window: moves its board into the lobby as a panel, and
 * closes the window without asking (the game goes on in the panel). */
export async function dockWindow() {
  if (session == null || isDocked) return;
  await api.setSessionDocked(session, true);
  await showLobby();
  docking = true;
  if (inTauri) {
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    await getCurrentWindow().destroy();
  } else {
    window.close();
  }
}

/** In a docked board: moves it to a window of its own, handing over
 * `handOff`. */
export function popOut(handOff: HandOff) {
  if (session == null || !isDocked) return;
  saveHandOff(session, handOff);
  askLobby({ kind: 'popOut' });
}
