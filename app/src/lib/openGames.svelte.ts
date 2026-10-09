// Every open game window's state, for the lobby's list and a game
// window's Next game: each session's latest view and followed
// arimaa.com game, kept up to date from the events every window gets.
import { api, apiFor } from './api';
import type { SessionId } from './bindings/SessionId';
import type { SessionView } from './bindings/SessionView';
import type { WatchView } from './bindings/WatchView';
import { on, onEvery } from './events';
import { waitsOnUser } from './windowList';

export class OpenGames {
  /** The open sessions, in the order they were opened. */
  ids = $state<SessionId[]>([]);
  /** Each session's latest view, and when it arrived (for its clock). */
  views = $state<Record<SessionId, { view: SessionView; at: number }>>({});
  /** Each session's followed arimaa.com game. */
  watches = $state<Record<SessionId, WatchView>>({});

  /** Starts following; returns the function that stops it. */
  follow(): () => void {
    const unlisteners = [
      on('sessions://changed', (s) => this.#setIds(s.sessions)),
      onEvery('game://changed', (u, id) => id != null && this.#track(id, u.view)),
      onEvery('gameroom://watch', (w, id) => {
        if (id == null) return;
        if (w.state === 'stopped') delete this.watches[id];
        else this.watches[id] = w;
      }),
    ];
    api.listSessions().then((ids) => this.#setIds(ids));
    return () => unlisteners.forEach((u) => void u.then((f) => f()));
  }

  /** The sessions whose game waits on the user's move. */
  waiting(): SessionId[] {
    return this.ids.filter((id) => this.views[id] && waitsOnUser(this.views[id].view));
  }

  /** The arimaa.com games open in a window (their gameroom ids). */
  openGids(): Set<string> {
    return new Set(Object.values(this.watches).map((w) => w.gid));
  }

  #track(id: SessionId, view: SessionView) {
    this.views[id] = { view, at: performance.now() };
    if (!this.ids.includes(id)) this.ids = [...this.ids, id];
  }

  async #add(id: SessionId) {
    const a = apiFor(id);
    try {
      const [view, watch] = await Promise.all([a.getState(), a.watchStatus()]);
      if (!this.views[id]) this.#track(id, view);
      if (watch && !this.watches[id]) this.watches[id] = watch;
    } catch {
      /* closed meanwhile */
    }
  }

  #setIds(list: SessionId[]) {
    this.ids = this.ids.filter((id) => list.includes(id));
    for (const id of Object.keys(this.views).map(Number)) {
      if (!list.includes(id)) {
        delete this.views[id];
        delete this.watches[id];
      }
    }
    for (const id of list) if (!this.views[id]) void this.#add(id);
  }
}
