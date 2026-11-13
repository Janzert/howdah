// The gameroom's chat room for the lobby: the backend's view of it
// (`chatroom://changed`), what the user has read, and their color. One
// instance, `chatroom`, in the lobby window.
import { api, errorMessage } from '../api';
import { CHAT_PALETTE, SPOILER_COLOR } from '../chatroom';
import type { ChatEventView } from '../bindings/ChatEventView';
import type { ChatRoomView } from '../bindings/ChatRoomView';
import { on } from '../events';
import { settings } from '../settings.svelte';
import { pref, savePref } from './store.svelte';

class ChatRoomStore {
  view = $state<ChatRoomView | null>(null);
  error = $state<string | null>(null);
  busy = $state(false);
  /** The message color, `rrggbb` (kept in localStorage). */
  color = $state(/^[0-9a-f]{6}$/.test(pref('gameroom.chatColor')) ? pref('gameroom.chatColor') : '000000');
  /** The user's own color, the palette's last slot (`rrggbb`, never the
   * spoiler's white; kept in localStorage). It starts as the palette's
   * last color, or a saved color from outside the palette. */
  custom = $state(initialCustom(this.color));
  /** The newest event the user has seen in the panel. */
  #seen = $state(-1);
  #started = false;
  /** What's before the room's own log, from the archive, oldest first,
   * and the events the room has dropped from its log since. Their ids
   * are the store's own, below zero; dropped events keep theirs. */
  older = $state<ChatEventView[]>([]);
  /** Bumped as earlier events go in front of `older`. */
  added = $state(0);
  earlierBusy = $state(false);
  earlierError = $state<string | null>(null);
  /** The start of the log was reached. */
  atStart = $state(false);
  /** The last search found nothing, back to this moment. */
  quietTo = $state<number | null>(null);
  /** Where the next search ends and what it leaves out; null before the
   * first, when that's the room's log. */
  #until: number | null = null;
  #skip = 0;
  #olderId = -1;
  /** A new one drops the answer to a search still going. */
  #visit = 0;
  /** Where the log is scrolled to, for the panel coming back to it; null
   * at its end. */
  scrollTop: number | null = null;
  /** The message being written and whether it goes as a spoiler, kept
   * while the panel is away (panels unmount). */
  draft = '';
  draftSpoiler = false;
  /** The newest event seen with the log at its end. */
  #readTo = $state(-1);

  inRoom = $derived(this.view?.state === 'in');
  /** Others' messages since the user last looked, while in the room. */
  unread = $derived(
    this.view?.state === 'in'
      ? this.view.events.filter((e) => e.id > this.#seen && e.kind === 'message' && e.user !== this.view?.user).length
      : 0,
  );
  /** Messages below where the log is scrolled back to. */
  newBelow = $derived(this.view?.events.filter((e) => e.id > this.#readTo && e.kind === 'message').length ?? 0);
  /** How many are in the room: its list while the user is in, else the
   * lobby's count. */
  count = $derived(this.view?.state === 'in' ? this.view.users.length : (this.view?.count ?? 0));

  /** Follows the backend's chat room and keeps its auto-leave setting in
   * step with the settings; once. */
  start(): () => void {
    if (this.#started) return () => {};
    this.#started = true;
    const unlisten = on('chatroom://changed', (v) => {
      // What the room sends on entering is its recent past, not news.
      if (v.state === 'in' && this.view?.state !== 'in') this.#seen = v.events.at(-1)?.id ?? -1;
      if (v.state === 'out') this.#resetEarlier();
      else this.#keepDropped(v);
      this.view = v;
    });
    api.chatroomState().then((v) => {
      if (this.view) return;
      if (v.state === 'in') this.#seen = v.events.at(-1)?.id ?? -1;
      this.view = v;
    });
    const stopSync = $effect.root(() => {
      $effect(() => {
        api.setChatroomAutoLeave(settings.leaveChatForGames).catch(() => {});
      });
    });
    return () => {
      this.#started = false;
      stopSync();
      void unlisten.then((f) => f());
    };
  }

  #resetEarlier() {
    this.older = [];
    this.earlierBusy = false;
    this.earlierError = null;
    this.atStart = false;
    this.quietTo = null;
    this.#until = null;
    this.#skip = 0;
    this.#visit++;
    this.scrollTop = null;
    this.#readTo = -1;
  }

  /** The log is at its end, showing everything. */
  readToEnd() {
    this.scrollTop = null;
    this.#readTo = this.view?.events.at(-1)?.id ?? -1;
  }

  /** With earlier events shown, what the room is about to drop from its
   * log moves to them, leaving no gap. */
  #keepDropped(next: ChatRoomView) {
    const first = next.events[0]?.id;
    if (this.#until === null || first === undefined) return;
    const dropped = this.view?.events.filter((e) => e.id < first) ?? [];
    if (dropped.length > 0) this.older = [...this.older, ...dropped];
  }

  /** Puts what's before the shown log in front of it. */
  async loadEarlier() {
    if (this.earlierBusy || this.atStart) return;
    if (this.#until === null) {
      // The log's first second may hold more events than it shows.
      const events = this.view?.events ?? [];
      const first = events[0]?.time;
      this.#until = first === undefined ? Math.floor(Date.now() / 1000) : first + 1;
      this.#skip = events.filter((e) => e.time === first).length;
    }
    const visit = this.#visit;
    this.earlierBusy = true;
    this.earlierError = null;
    try {
      const r = await api.chatArchiveEarlier(this.#until, this.#skip);
      if (visit !== this.#visit) return;
      this.#until = r.from;
      this.#skip = 0;
      this.atStart = r.atStart;
      this.quietTo = r.events.length > 0 ? null : r.from;
      this.older = [...r.events.map((e) => ({ ...e, id: this.#olderId-- })), ...this.older];
      this.added++;
    } catch (e) {
      if (visit === this.#visit) this.earlierError = errorMessage(e);
    } finally {
      if (visit === this.#visit) this.earlierBusy = false;
    }
  }

  /** Marks everything shown as read. */
  markSeen() {
    const last = this.view?.events.at(-1);
    if (last && last.id > this.#seen) this.#seen = last.id;
  }

  setColor(hex: string) {
    this.color = hex;
    savePref('gameroom.chatColor', hex);
  }

  /** Sets the user's own color and uses it. */
  setCustom(hex: string) {
    if (hex === SPOILER_COLOR) return;
    this.custom = hex;
    savePref('gameroom.chatCustomColor', hex);
    this.setColor(hex);
  }

  async #run(f: () => Promise<unknown>): Promise<boolean> {
    this.busy = true;
    this.error = null;
    try {
      await f();
      return true;
    } catch (e) {
      this.error = errorMessage(e);
      return false;
    } finally {
      this.busy = false;
    }
  }

  enter() {
    return this.#run(async () => (this.view = await api.enterChatroom(settings.leaveChatForGames)));
  }

  leave() {
    return this.#run(() => api.leaveChatroom());
  }

  /** Says `text` in the user's color, or as a spoiler (in white, the
   * room's spoiler tag). */
  say(text: string, spoiler = false) {
    return this.#run(() => api.sayInChatroom(text, spoiler ? SPOILER_COLOR : this.color));
  }
}

function initialCustom(color: string): string {
  const saved = pref('gameroom.chatCustomColor');
  if (/^[0-9a-f]{6}$/.test(saved) && saved !== SPOILER_COLOR) return saved;
  if (!CHAT_PALETTE.includes(color) && color !== SPOILER_COLOR) return color;
  return CHAT_PALETTE[CHAT_PALETTE.length - 1];
}

export const chatroom = new ChatRoomStore();
