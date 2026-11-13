// The arimaa.com player pages' state: for each player whose page is open,
// what their page says, their rating history, games and record by
// opponent. Each page is its own panel (`player:<id>`) and stays that
// player; opening a player whose page is open shows that panel.

import { api, errorMessage } from '../api';
import type { PastGameView } from '../bindings/PastGameView';
import type { PlayerProfileView } from '../bindings/PlayerProfileView';
import type { PlayerRecordView } from '../bindings/PlayerRecordView';
import type { RatingPointView } from '../bindings/RatingPointView';
import { gameroom, pref, savePref } from './store.svelte';

/** A player to show: the id their page is asked for by, and the username
 * their rating history is. */
export interface PlayerRef {
  id: string;
  username: string;
}

/** Which games a record counts: `null` for all. */
export interface RecordFilter {
  rated: boolean | null;
  bots: boolean | null;
}

/** A part of the page fetched on its own, with its error. */
interface Part<T> {
  value: T | null;
  loading: boolean;
  error: string | null;
}

const empty = <T>(): Part<T> => ({ value: null, loading: false, error: null });

export class PlayerPage {
  /** The player shown, for good. */
  readonly player: PlayerRef;
  profile = $state<Part<PlayerProfileView>>(empty());
  history = $state<Part<RatingPointView[]>>(empty());
  /** Their finished games, newest first, and the offset of the next page. */
  games = $state<Part<{ games: PastGameView[]; next: number | null }>>(empty());
  record = $state<Part<PlayerRecordView>>(empty());
  recordFilter = $state<RecordFilter>({ rated: null, bots: null });
  /** The opponent whose games against the player are open in the record,
   * and those games. */
  againstId = $state<string | null>(null);
  against = $state<Part<PastGameView[]>>(empty());
  /** The section shown under the graph. */
  tab = $state<'games' | 'record'>('games');

  /** Counts the loads, so replies for an earlier one are dropped. */
  #shown = 0;

  constructor(player: PlayerRef) {
    this.player = player;
    this.#load();
  }

  /** Fetches the page again. */
  reload() {
    this.#load(this.tab);
  }

  #load(tab: 'games' | 'record' = 'games') {
    const shown = ++this.#shown;
    const player = this.player;
    this.tab = tab;
    this.profile = empty();
    this.history = empty();
    this.games = empty();
    this.record = empty();
    this.againstId = null;
    this.against = empty();
    // One after another: the requests are spaced a second apart anyway,
    // and the page comes first.
    void (async () => {
      await this.#fetch(shown, 'profile', () => api.gameroomPlayerProfile(player.id));
      await this.#fetch(shown, 'history', () => api.gameroomRatingHistory(player.username));
      if (this.tab === 'record') await this.loadRecord();
      else await this.#fetch(shown, 'games', () => api.gameroomPlayerGames(player.id, 0));
    })();
  }

  async #fetch<K extends 'profile' | 'history' | 'games' | 'record'>(
    shown: number,
    part: K,
    get: () => Promise<NonNullable<this[K]['value']>>,
  ) {
    if (shown !== this.#shown) return;
    (this[part] as Part<unknown>) = { value: null, loading: true, error: null };
    try {
      const value = await get();
      if (shown === this.#shown) (this[part] as Part<unknown>) = { value, loading: false, error: null };
    } catch (e) {
      if (shown === this.#shown)
        (this[part] as Part<unknown>) = { value: null, loading: false, error: errorMessage(e) };
    }
  }

  /** Shows the games, fetching them the first time. */
  showGames() {
    this.tab = 'games';
    const player = this.player;
    if (!this.games.value && !this.games.loading)
      void this.#fetch(this.#shown, 'games', () => api.gameroomPlayerGames(player.id, 0));
  }

  /** Fetches the next page of games. */
  async olderGames() {
    const player = this.player;
    const page = this.games.value;
    if (!page || page.next == null || this.games.loading) return;
    const shown = this.#shown;
    this.games = { ...this.games, loading: true, error: null };
    try {
      const more = await api.gameroomPlayerGames(player.id, page.next);
      if (shown === this.#shown)
        this.games = { value: { games: [...page.games, ...more.games], next: more.next }, loading: false, error: null };
    } catch (e) {
      if (shown === this.#shown) this.games = { ...this.games, loading: false, error: errorMessage(e) };
    }
  }

  /** Shows the record by opponent, fetching it the first time. */
  showRecord() {
    this.tab = 'record';
    if (!this.record.value && !this.record.loading) void this.loadRecord();
  }

  /** Counts only the games `filter` says, fetching the record again. */
  setRecordFilter(filter: RecordFilter) {
    this.recordFilter = filter;
    this.againstId = null;
    this.against = empty();
    void this.loadRecord();
  }

  loadRecord() {
    const player = this.player;
    const { rated, bots } = this.recordFilter;
    return this.#fetch(this.#shown, 'record', () => api.gameroomPlayerRecord(player.id, rated, bots));
  }

  /** Shows the games against `opponentId` (again: hides them). */
  async toggleAgainst(opponentId: string) {
    const player = this.player;
    this.against = empty();
    if (this.againstId === opponentId) {
      this.againstId = null;
      return;
    }
    this.againstId = opponentId;
    const { rated, bots } = this.recordFilter;
    const shown = this.#shown;
    // A reply for an earlier load, or another opponent opened since, is dropped.
    const current = () => shown === this.#shown && this.againstId === opponentId;
    this.against = { value: null, loading: true, error: null };
    try {
      const games = await api.gameroomGamesAgainst(player.id, opponentId, rated, bots);
      if (current()) this.against = { value: games, loading: false, error: null };
    } catch (e) {
      if (current()) this.against = { value: null, loading: false, error: errorMessage(e) };
    }
  }
}

const NAMES_KEY = 'lobby.playerPanels';

function readNames(): Record<string, string> {
  try {
    const v = JSON.parse(pref(NAMES_KEY) || '{}');
    return v && typeof v === 'object' && !Array.isArray(v) ? v : {};
  } catch {
    return {};
  }
}

/** The open player pages: each one's username (kept in localStorage, so
 * the sidebar names them after a restart) and, once shown, its page. */
class PlayerPages {
  /** Usernames by player id, for every page open. */
  names = $state<Record<string, string>>(readNames());
  #pages = new Map<string, PlayerPage>();

  /** The page of player `id`, fetched the first time it's asked for;
   * null if no page of theirs is open. */
  page(id: string): PlayerPage | null {
    let page = this.#pages.get(id);
    if (!page) {
      const username = this.names[id];
      if (username == null) return null;
      page = new PlayerPage({ id, username });
      this.#pages.set(id, page);
    }
    return page;
  }

  /** The username of player `id`, if their page is open. */
  username(id: string): string | null {
    return this.names[id] ?? null;
  }

  /** Notes `player`'s page as open. */
  add(player: PlayerRef) {
    if (this.names[player.id] === player.username) return;
    this.names = { ...this.names, [player.id]: player.username };
    savePref(NAMES_KEY, JSON.stringify(this.names));
  }

  /** Forgets player `id`'s page, when its panel closes. */
  remove(id: string) {
    this.#pages.delete(id);
    if (!(id in this.names)) return;
    const { [id]: _, ...rest } = this.names;
    this.names = rest;
    savePref(NAMES_KEY, JSON.stringify(rest));
  }
}

export const playerPages = new PlayerPages();

/** Shows a player's page in its own arimaa.com panel: the one already open
 * for them, or a new one. */
export function showPlayerPage(player: PlayerRef) {
  playerPages.add(player);
  gameroom.show(`player:${player.id}`);
}
