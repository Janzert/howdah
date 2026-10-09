// Game windows (docs/WINDOWS.md): each window shows its own session, named
// in its URL (`?session=<id>`; the main window has none and shows
// MAIN_SESSION). In Tauri a game window's label is `game-<session>`, and
// the backend ends the session when the window is destroyed
// (`Backend::window_closed`). In the browser preview a window is a tab,
// and it closes its session itself on `pagehide`.
import { api, isGameWindow, session } from './api';

export const inTauri = '__TAURI_INTERNALS__' in window;

/** Opens a new game window on a new session with an empty game. */
export async function openGameWindow(): Promise<void> {
  const id = await api.openSession();
  const url = `index.html?session=${id}`;
  try {
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
    } else if (!window.open(url)) {
      throw new Error("Couldn't open a window (the browser blocked it)");
    }
  } catch (e) {
    await api.closeSession(id).catch(() => {});
    throw e;
  }
}

/** Asks before the window closes while `warning()` gives a reason to (a
 * game or match that closing would end): `ask(reason)` shows it and
 * resolves to whether to close anyway. In Tauri the backend then ends
 * the session as the window goes; the browser preview can only show its
 * own generic prompt. */
export async function guardClose(warning: () => string | null, ask: (reason: string) => Promise<boolean>) {
  if (!inTauri) {
    window.addEventListener('beforeunload', (e) => {
      if (isGameWindow && warning()) e.preventDefault();
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
  window.addEventListener('pagehide', () => closeSessionOnUnload(session));
}
