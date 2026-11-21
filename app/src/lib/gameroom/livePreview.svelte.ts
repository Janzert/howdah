// The live game shown small beside the chat room's log: one arimaa.com
// game at a time, followed on a session of its own with no board (one
// viewer seat, as watching it on a board takes). A game that already has a
// board is shown from that board's session instead, taking no seat. The
// lobby leaves the preview's own session out of its open boards, and
// Watch on the same game turns it into a board (`release`). Its moves are
// animated as on a board (`model`), except while the lobby is hidden,
// when it jumps to each new position instead.
import { BoardModel } from '@howdah/board/board/boardModel.svelte';
import { api, apiFor, errorMessage } from '../api';
import type { SessionId } from '../bindings/SessionId';
import type { SessionUpdate } from '../bindings/SessionUpdate';
import type { SessionView } from '../bindings/SessionView';
import type { WatchView } from '../bindings/WatchView';
import { onEvery } from '../events';
import { gameroom } from './store.svelte';

/** Where the preview's own session is noted, so a lobby that reloads (the
 * browser preview) closes the one it left behind. */
const SESSION_KEY = 'chat.previewSession';

function note(id: SessionId | null) {
  try {
    if (id == null) localStorage.removeItem(SESSION_KEY);
    else localStorage.setItem(SESSION_KEY, String(id));
  } catch {
    /* only the leftover check misses it */
  }
}

class LivePreview {
  /** The game shown, by its gameroom id. */
  gid = $state<string | null>(null);
  /** The session it's shown from, once there is one. */
  session = $state<SessionId | null>(null);
  /** Whether that session is the preview's own (closed with it), rather
   * than a board's. */
  owned = $state(false);
  view = $state<SessionView | null>(null);
  /** When `view` arrived (`performance.now()`), for its clocks. */
  at = $state(0);
  watch = $state<WatchView | null>(null);
  /** The pieces as shown, played from one update to the next. */
  model = new BoardModel();
  opening = $state(false);
  error = $state<string | null>(null);

  #started = false;
  /** Whether `view` came from the current session (not one kept from the
   * board it was shown from before). */
  #fresh = false;
  /** Bumped by every expand and close, so a slow open that's been
   * overtaken closes what it opened. */
  #generation = 0;

  /** The preview's own session, for the lobby to leave out of its boards. */
  get ownSession(): SessionId | null {
    return this.owned ? this.session : null;
  }

  /** Follows the sessions' events; once. A session left by an earlier
   * lobby is closed. */
  start() {
    if (this.#started) return;
    this.#started = true;
    try {
      const left = Number(localStorage.getItem(SESSION_KEY));
      if (left) void api.closeSession(left).catch(() => {});
    } catch {
      /* nothing to close */
    }
    note(null);
    void onEvery('game://changed', (u, id) => {
      if (id != null && id === this.session) this.#update(u);
    });
    // An animation the hidden page left half played is skipped.
    document.addEventListener('visibilitychange', () => {
      if (!document.hidden && this.view && this.model.animating) this.model.jump(this.view.position.pieces);
    });
    void onEvery('gameroom://watch', (w, id) => {
      if (id != null && id === this.session) this.watch = w;
    });
    void onEvery('sessions://changed', (s) => {
      if (this.session == null || s.sessions.includes(this.session)) return;
      // The board it was shown from closed: a game still being played
      // goes on here on a session of its own, the position kept meanwhile.
      const gid = this.gid;
      const live = gameroom.games?.live.some((g) => g.gid === gid) ?? true;
      if (gid && !this.owned && !this.view?.result && live) void this.#open(gid, true);
      else this.#forget();
    });
  }

  /** Shows game `gid`, or hides it if it's the one shown. */
  async toggle(gid: string) {
    if (this.gid === gid) return this.close();
    await this.close();
    await this.#open(gid, false);
  }

  /** Follows game `gid`: from a board's session showing it, or else on
   * one of its own. `keep` leaves the position shown until the new
   * session's arrives. */
  async #open(gid: string, keep: boolean) {
    const generation = ++this.#generation;
    this.gid = gid;
    this.session = null;
    this.owned = false;
    this.watch = null;
    this.#fresh = false;
    if (!keep) this.view = null;
    this.opening = true;
    this.error = null;
    try {
      const existing = await api.gameroomGameSession(gid).catch(() => null);
      if (generation !== this.#generation) return;
      if (existing != null) {
        await this.#show(existing, false, keep);
        return;
      }
      const id = await api.openSession();
      if (generation !== this.#generation) {
        void api.closeSession(id).catch(() => {});
        return;
      }
      this.session = id;
      this.owned = true;
      note(id);
      await apiFor(id).openGameroomGame(gid);
      if (generation !== this.#generation) return;
      await this.#show(id, true, keep);
    } catch (e) {
      if (generation !== this.#generation) return;
      this.error = errorMessage(e);
      if (this.owned && this.session != null) void api.closeSession(this.session).catch(() => {});
      this.session = null;
      this.owned = false;
      note(null);
    } finally {
      if (generation === this.#generation) this.opening = false;
    }
  }

  async #show(id: SessionId, owned: boolean, kept: boolean) {
    this.session = id;
    this.owned = owned;
    const a = apiFor(id);
    const [view, watch] = await Promise.all([a.getState(), a.watchStatus()]);
    if (this.session !== id) return;
    // An event may have brought a newer view meanwhile; a kept one is the
    // old session's.
    if (!this.view || (kept && !this.#fresh)) {
      this.view = view;
      this.at = performance.now();
      this.model.jump(view.position.pieces);
    }
    this.watch ??= watch;
  }

  #update(u: SessionUpdate) {
    const first = this.view == null || !this.#fresh;
    this.#fresh = true;
    this.view = u.view;
    this.at = performance.now();
    const pieces = u.view.position.pieces;
    if (first || document.hidden) this.model.jump(pieces);
    else this.model.apply(pieces, u.animation, {}, u.animationBudgetMs);
  }

  /** Hides the game, closing the preview's own session (its seat). */
  async close() {
    this.#generation++;
    const own = this.ownSession;
    this.#forget();
    if (own != null) await api.closeSession(own).catch(() => {});
  }

  /** Hands the preview's own session `id` over to become a board: it's
   * still shown here, but no longer closed with the preview. */
  release(id: SessionId) {
    if (this.session === id && this.owned) {
      this.owned = false;
      note(null);
    }
  }

  #forget() {
    if (this.owned) note(null);
    this.gid = null;
    this.session = null;
    this.owned = false;
    this.view = null;
    this.watch = null;
    this.#fresh = false;
    this.opening = false;
    this.error = null;
  }
}

export const livePreview = new LivePreview();
