// The arimaa.com player page's state: the player shown, what their page
// says, their rating history, games and record by opponent. One player at
// a time, with the players shown before it to go back to.

import { api, errorMessage } from '../api';
import type { PastGameView } from '../bindings/PastGameView';
import type { PlayerProfileView } from '../bindings/PlayerProfileView';
import type { PlayerRecordView } from '../bindings/PlayerRecordView';
import type { RatingPointView } from '../bindings/RatingPointView';
import { gameroom } from './store.svelte';

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

/** How many players Back remembers. */
const BACK_LIMIT = 20;

class PlayerPage {
  /** The player shown. */
  player = $state<PlayerRef | null>(null);
  /** The players shown before, newest last, for Back. */
  back = $state<PlayerRef[]>([]);
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

  /** Counts the players shown, so replies for an earlier one are dropped. */
  #shown = 0;

  /** Shows `player`'s page in the player panel, fetching it afresh. */
  show(player: PlayerRef) {
    const current = this.player;
    if (current && current.id !== player.id) this.back = [...this.back, current].slice(-BACK_LIMIT);
    this.#load(player);
    gameroom.show('player');
  }

  /** Goes back to the player shown before this one. */
  goBack() {
    const previous = this.back.at(-1);
    if (!previous) return;
    this.back = this.back.slice(0, -1);
    this.#load(previous);
  }

  /** Fetches the page again. */
  reload() {
    if (this.player) this.#load(this.player, this.tab);
  }

  #load(player: PlayerRef, tab: 'games' | 'record' = 'games') {
    const shown = ++this.#shown;
    this.player = player;
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
    if (player && !this.games.value && !this.games.loading)
      void this.#fetch(this.#shown, 'games', () => api.gameroomPlayerGames(player.id, 0));
  }

  /** Fetches the next page of games. */
  async olderGames() {
    const player = this.player;
    const page = this.games.value;
    if (!player || !page || page.next == null || this.games.loading) return;
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
    if (!player) return Promise.resolve();
    const { rated, bots } = this.recordFilter;
    return this.#fetch(this.#shown, 'record', () => api.gameroomPlayerRecord(player.id, rated, bots));
  }

  /** Shows the games against `opponentId` (again: hides them). */
  async toggleAgainst(opponentId: string) {
    const player = this.player;
    if (!player) return;
    this.against = empty();
    if (this.againstId === opponentId) {
      this.againstId = null;
      return;
    }
    this.againstId = opponentId;
    const { rated, bots } = this.recordFilter;
    const shown = this.#shown;
    // A reply for another player, or another opponent opened since, is dropped.
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

export const playerPage = new PlayerPage();

/** Shows a player's page in the arimaa.com player panel. */
export function showPlayerPage(player: PlayerRef) {
  playerPage.show(player);
}
