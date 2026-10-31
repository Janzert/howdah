// A board opened before its game has loaded (from arimaa.com or the test
// daemon, which may take a while): whoever opens it says so here, and
// the board shows a loading overlay, or the error if loading failed,
// until it's cleared. Kept in localStorage, whose `storage` events reach
// every window and docked board (not the one that wrote it).
import type { SessionId } from './bindings/SessionId';

/** What a board waits for: `text` while loading, `error` once it failed. */
export type BoardLoading = { text: string; error?: string };

const PREFIX = 'boardLoading.';

/** This document's listeners for any board's state (`followAll`):
 * `storage` events don't reach the document that wrote. */
const local = new Set<() => void>();
const changed = () => local.forEach((f) => f());
const key = (id: SessionId) => `${PREFIX}${id}`;

export function setBoardLoading(id: SessionId, state: BoardLoading) {
  try {
    localStorage.setItem(key(id), JSON.stringify(state));
  } catch {
    /* the board just won't say */
  }
  changed();
}

export function clearBoardLoading(id: SessionId) {
  try {
    localStorage.removeItem(key(id));
  } catch {
    /* nothing kept */
  }
  changed();
}

export function readBoardLoading(id: SessionId): BoardLoading | null {
  try {
    const raw = localStorage.getItem(key(id));
    return raw ? (JSON.parse(raw) as BoardLoading) : null;
  } catch {
    return null;
  }
}

/** Follows board `id`'s loading state; returns the function that stops. */
export function followBoardLoading(id: SessionId, onChange: (state: BoardLoading | null) => void): () => void {
  const listener = (e: StorageEvent) => {
    if (e.key === key(id) || e.key === null) onChange(readBoardLoading(id));
  };
  window.addEventListener('storage', listener);
  return () => window.removeEventListener('storage', listener);
}

/** Drops every board's state (the lobby, as the app starts: session ids
 * start again from 1). */
export function clearAllBoardLoading() {
  try {
    for (const k of Object.keys(localStorage)) if (k.startsWith(PREFIX)) localStorage.removeItem(k);
  } catch {
    /* nothing kept */
  }
}

/** Every board's state, by session. */
export function readAllBoardLoading(): Record<SessionId, BoardLoading> {
  const all: Record<SessionId, BoardLoading> = {};
  try {
    for (const k of Object.keys(localStorage)) {
      if (!k.startsWith(PREFIX)) continue;
      const id = Number(k.slice(PREFIX.length));
      const state = readBoardLoading(id);
      if (state) all[id] = state;
    }
  } catch {
    /* none */
  }
  return all;
}

/** Follows every board's state (the lobby's list); returns the function
 * that stops. */
export function followAllBoardLoading(onChange: (all: Record<SessionId, BoardLoading>) => void): () => void {
  const update = () => onChange(readAllBoardLoading());
  const listener = (e: StorageEvent) => {
    if (e.key === null || e.key.startsWith(PREFIX)) update();
  };
  window.addEventListener('storage', listener);
  local.add(update);
  return () => {
    window.removeEventListener('storage', listener);
    local.delete(update);
  };
}
