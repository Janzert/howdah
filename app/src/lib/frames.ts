// Boards docked in the lobby window (docs/WINDOWS.md, "Boards as panels"):
// each is the game window's page (`App.svelte`) in an iframe,
// `index.html?session=<id>&docked`. Tauri gives its IPC only to a
// window's main frame, so a docked board's `invoke` and events go
// through the lobby: the frame's IPC is mocked (`installFrameRelay`) to
// post each command to the lobby, which runs it and posts back the
// result, and the lobby posts every event on to every frame
// (`serveFrames`). Window commands (`plugin:…`) aren't relayed; what a
// board does to windows (open a board, pop out, close) it asks the lobby
// for (`askLobby`), which does it as a panel or a window.
import { invoke } from '@tauri-apps/api/core';
import { emit, listen } from '@tauri-apps/api/event';
import { mockIPC } from '@tauri-apps/api/mocks';
import type { ApiError } from './bindings/ApiError';
import type { SessionId } from './bindings/SessionId';
import { EVENT_NAMES } from './events';

/** What a docked board asks of the lobby. */
export type FrameRequest =
  /** Show session `session` (a new board, or Next game's): a panel or a
   * window, as the Settings say for a new one (`fresh`). */
  | { kind: 'show'; session: SessionId; fresh?: boolean }
  /** Move this board to a window of its own. */
  | { kind: 'popOut' }
  /** Close this board (its own close question, if any, was answered). */
  | { kind: 'close' }
  /** A lobby key pressed in the board (`lobbyKey`). */
  | { kind: 'key'; key: LobbyKey };

/** The lobby's own keys, which a docked board passes on: Alt+↑/↓ for the
 * previous or next sidebar entry, Alt+1–9 for one by number. */
export type LobbyKey = { step: 1 | -1 } | { index: number };

/** The lobby key a key press means, if any. */
export function lobbyKey(e: Pick<KeyboardEvent, 'key' | 'altKey' | 'ctrlKey' | 'metaKey' | 'shiftKey'>): LobbyKey | null {
  if (!e.altKey || e.ctrlKey || e.metaKey || e.shiftKey) return null;
  if (e.key === 'ArrowUp') return { step: -1 };
  if (e.key === 'ArrowDown') return { step: 1 };
  if (/^[1-9]$/.test(e.key)) return { index: Number(e.key) - 1 };
  return null;
}

type ToLobby =
  | { howdah: 'invoke'; id: number; cmd: string; args: unknown }
  | { howdah: 'request'; request: FrameRequest };

type ToFrame =
  | { howdah: 'reply'; id: number; ok: boolean; value: unknown }
  | { howdah: 'event'; name: string; payload: unknown }
  /** The lobby's close button: run the close question, then `close`. */
  | { howdah: 'requestClose' };

const mine = (e: MessageEvent) => e.origin === location.origin;

// ---- In a docked board ----

let nextId = 1;
const pending = new Map<number, { resolve: (v: unknown) => void; reject: (e: unknown) => void }>();
let onRequestClose: (() => void) | null = null;

function post(message: ToLobby) {
  window.parent.postMessage(message, location.origin);
}

/** In a docked board, before anything invokes or listens: routes both
 * through the lobby. */
export function installFrameRelay() {
  mockIPC(
    (cmd, args) =>
      new Promise((resolve, reject) => {
        const id = nextId++;
        pending.set(id, { resolve, reject });
        post({ howdah: 'invoke', id, cmd, args });
      }),
    { shouldMockEvents: true },
  );
  window.addEventListener('message', (e: MessageEvent<ToFrame>) => {
    if (!mine(e) || e.source !== window.parent) return;
    const m = e.data;
    if (m?.howdah === 'reply') {
      const p = pending.get(m.id);
      pending.delete(m.id);
      if (m.ok) p?.resolve(m.value);
      else p?.reject(m.value);
    } else if (m?.howdah === 'event') {
      void emit(m.name, m.payload);
    } else if (m?.howdah === 'requestClose') {
      if (onRequestClose) onRequestClose();
      else post({ howdah: 'request', request: { kind: 'close' } });
    }
  });
}

/** In a docked board, asks the lobby to do something with windows. */
export function askLobby(request: FrameRequest) {
  post({ howdah: 'request', request });
}

/** In a docked board, what the lobby's close button runs (the board's
 * close question, then `askLobby({ kind: 'close' })`). */
export function onCloseRequest(handler: () => void) {
  onRequestClose = handler;
}

// ---- In the lobby ----

const refused = (cmd: string): ApiError => ({
  kind: 'state',
  message: `A board in the lobby can't run ${cmd}`,
  line: null,
});

/** In the lobby: answers the docked boards' commands and passes every
 * event on to them. `frames()` gives each docked board's session and
 * frame; `onRequest` does what a board asks. Returns the function that
 * stops it. */
export function serveFrames(
  frames: () => { session: SessionId; frame: HTMLIFrameElement }[],
  onRequest: (session: SessionId, request: FrameRequest) => void,
): () => void {
  const sessionOf = (source: MessageEventSource | null) =>
    frames().find((f) => f.frame.contentWindow === source)?.session ?? null;

  async function onMessage(e: MessageEvent<ToLobby>) {
    if (!mine(e)) return;
    const session = sessionOf(e.source);
    const m = e.data;
    if (session == null || !m?.howdah) return;
    const reply = (ok: boolean, value: unknown) =>
      (e.source as Window).postMessage({ howdah: 'reply', id: (m as { id: number }).id, ok, value } satisfies ToFrame, location.origin);
    if (m.howdah === 'invoke') {
      if (m.cmd.startsWith('plugin:')) return reply(false, refused(m.cmd));
      try {
        reply(true, await invoke(m.cmd, m.args as Record<string, unknown>));
      } catch (err) {
        reply(false, err);
      }
    } else if (m.howdah === 'request') {
      onRequest(session, m.request);
    }
  }
  window.addEventListener('message', onMessage);
  const unlisteners = EVENT_NAMES.map((name) =>
    listen(name, (e) => {
      for (const f of frames()) {
        f.frame.contentWindow?.postMessage({ howdah: 'event', name, payload: e.payload } satisfies ToFrame, location.origin);
      }
    }),
  );
  return () => {
    window.removeEventListener('message', onMessage);
    unlisteners.forEach((u) => void u.then((f) => f()));
  };
}

/** In the lobby: asks a docked board to close (it may ask first). */
export function requestClose(frame: HTMLIFrameElement) {
  frame.contentWindow?.postMessage({ howdah: 'requestClose' } satisfies ToFrame, location.origin);
}
