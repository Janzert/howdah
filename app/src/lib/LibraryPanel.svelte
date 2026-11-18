<script lang="ts">
  // The lobby's Games panel: the game database's games, newest first,
  // narrowed by players, dates, where they're from, rated and how they
  // ended. Each opens on a board of its own; Delete takes one out of the
  // database (a second click confirms).
  import { onDestroy } from 'svelte';
  import { api, errorMessage } from './api';
  import type { BaseProgress } from './bindings/BaseProgress';
  import type { BaseStatus } from './bindings/BaseStatus';
  import { on } from './events';
  import type { LibraryGameView } from './bindings/LibraryGameView';
  import Icon from './Icon.svelte';
  import Matchup from './gameroom/Matchup.svelte';
  import { EMPTY_FORM, ENDINGS, libraryMeta, toFilter, type FilterForm } from './library';
  import { pref, savePref } from './gameroom/store.svelte';

  interface Props {
    /** Opens a game; resolves to an error message, or null. */
    onOpen: (id: number) => Promise<string | null>;
  }
  let { onOpen }: Props = $props();

  const PAGE = 50;

  function readForm(): FilterForm {
    try {
      return { ...EMPTY_FORM, ...JSON.parse(pref('lobby.gamesFilter') || '{}') };
    } catch {
      return { ...EMPTY_FORM };
    }
  }

  let form = $state<FilterForm>(readForm());
  let games = $state<LibraryGameView[]>([]);
  let total = $state<number | null>(null);
  let available = $state(true);
  let error = $state<string | null>(null);
  let loading = $state(false);
  let opening = $state<number | null>(null);
  let confirming = $state<number | null>(null);
  /** Counts loads, so a slow one that's been overtaken is dropped. */
  let generation = 0;

  async function load(more = false) {
    const g = ++generation;
    loading = true;
    error = null;
    try {
      const page = await api.libraryGames(toFilter(form), more ? games.length : 0, PAGE);
      if (g !== generation) return;
      games = more ? [...games, ...page.games] : page.games;
      total = page.total;
      available = page.available;
    } catch (e) {
      if (g === generation) error = errorMessage(e);
    } finally {
      if (g === generation) loading = false;
    }
  }

  // Reloads a moment after the filter stops changing.
  let timer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    const snapshot = JSON.stringify(form);
    savePref('lobby.gamesFilter', snapshot);
    clearTimeout(timer);
    timer = setTimeout(() => void load(), 250);
    return () => clearTimeout(timer);
  });

  async function open(id: number) {
    opening = id;
    error = await onOpen(id);
    opening = null;
  }

  async function remove(id: number) {
    if (confirming !== id) {
      confirming = id;
      return;
    }
    confirming = null;
    try {
      await api.deleteLibraryGame(id);
      games = games.filter((g) => g.id !== id);
      if (total != null) total -= 1;
    } catch (e) {
      error = errorMessage(e);
    }
  }

  // The base database (arimaa.com's archive): offered when there's a newer
  // one than the database has, with its download's progress.
  let base = $state<BaseStatus | null>(null);
  let progress = $state<BaseProgress | null>(null);
  let baseError = $state<string | null>(null);
  let baseDone = $state<string | null>(null);

  async function loadBase() {
    try {
      base = await api.baseStatus();
    } catch {
      base = null;
    }
  }
  void loadBase();

  const stop = on('library://base', (p) => {
    progress = p.phase === 'done' || p.phase === 'failed' ? null : p;
  });
  onDestroy(() => void stop.then((u) => u()));

  async function installBase() {
    baseError = null;
    baseDone = null;
    progress = { phase: 'downloading', done: 0, total: base?.available?.size ?? 0, detail: null };
    try {
      baseDone = await api.installBase();
      void load();
    } catch (e) {
      baseError = errorMessage(e);
    }
    progress = null;
    void loadBase();
  }

  const mb = (n: number) => `${Math.round(n / 1e6)} MB`;
  const progressText = $derived.by(() => {
    switch (progress?.phase) {
      case 'downloading':
        return `Downloading… ${mb(progress.done)} of ${mb(progress.total)}`;
      case 'checking':
        return 'Checking the download…';
      case 'unpacking':
        return 'Unpacking…';
      case 'merging':
        return 'Adding the games to yours (this can take a minute)…';
      default:
        return '';
    }
  });

  const filtered = $derived(JSON.stringify(form) !== JSON.stringify(EMPTY_FORM));
  const countText = $derived(
    total == null ? '' : `${total.toLocaleString()} ${total === 1 ? 'game' : 'games'}${filtered ? ' match' : ''}`,
  );
</script>

<div class="screen">
  <header>
    <h1>Games</h1>
    <span class="count" aria-live="polite">{countText}</span>
    <span class="spacer"></span>
    <button class="icon" onclick={() => load()} title="Load the list again" aria-label="Refresh"
      ><Icon name="refresh" size={16} /></button
    >
  </header>
  <p class="hint intro">
    Every game you play, watch, analyse or open is kept here, with arimaa.com's games you've opened. Each opens on a
    board of its own, and what you add to it is saved with it.
  </p>

  {#if base?.available && (base.offer || progress || base.installing || baseError)}
    {@const a = base.available}
    <section class="card base" aria-label="arimaa.com's game archive">
      <p>
        {#if base.installed && a.newGames != null}
          {a.newGames.toLocaleString()} new {a.newGames === 1 ? 'game' : 'games'} from arimaa.com's archive{a.through
            ? `, through ${a.through}`
            : ''}.
        {:else if base.installed}
          A newer copy of arimaa.com's game archive is out: {a.games.toLocaleString()} games{a.through
            ? ` through ${a.through}`
            : ''}, where yours goes through {base.installed.through ?? 'an earlier date'}.
        {:else}
          Get arimaa.com's game archive: {a.games.toLocaleString()} games{a.through ? ` through ${a.through}` : ''}, to
          search and open here, offline too.
        {/if}
        {#if a.size}<span class="size">{a.size < 1e6 ? 'Under 1 MB' : mb(a.size)} to download.</span>{/if}
      </p>
      {#if progress || base.installing}
        <div class="progress" role="status" aria-live="polite">
          {#if progress?.phase === 'downloading' && progress.total > 0}
            <progress max={progress.total} value={progress.done}></progress>
          {:else}
            <progress></progress>
          {/if}
          <span>{progressText || 'Installing…'}</span>
        </div>
      {:else}
        <button class="primary" onclick={installBase}>{base.installed ? 'Update' : 'Download'}</button>
      {/if}
      {#if baseError}<p class="error" role="alert">{baseError}</p>{/if}
    </section>
  {:else if baseDone}
    <p class="hint done" role="status">arimaa.com's game archive is in: {baseDone}.</p>
  {/if}

  <form class="filters" aria-label="Filter games" onsubmit={(e) => e.preventDefault()}>
    <label>Player <input aria-label="Player" bind:value={form.player} autocomplete="off" spellcheck="false" /></label>
    <label>
      Side
      <select aria-label="Side" bind:value={form.side} disabled={!form.player.trim()}>
        <option value="">Either</option>
        <option value="gold">Gold</option>
        <option value="silver">Silver</option>
      </select>
    </label>
    <label>
      Result
      <select aria-label="Result" bind:value={form.outcome} disabled={!form.player.trim()}>
        <option value="">Any</option>
        <option value="won">Won</option>
        <option value="lost">Lost</option>
      </select>
    </label>
    <label title="With no player, either side">
      Opponent <input aria-label="Opponent" bind:value={form.opponent} autocomplete="off" spellcheck="false" />
    </label>
    <label>
      Ending
      <select aria-label="Ending" bind:value={form.ending}>
        <option value="">Any</option>
        {#each ENDINGS as [letter, word] (letter)}<option value={letter}>{word}</option>{/each}
      </select>
    </label>
    <label>From <input type="date" aria-label="From" bind:value={form.from} /></label>
    <label>To <input type="date" aria-label="To" bind:value={form.to} /></label>
    <label>
      Source
      <select aria-label="Source" bind:value={form.origin}>
        <option value="">Anywhere</option>
        <option value="arimaa">arimaa.com</option>
        <option value="local">This app</option>
      </select>
    </label>
    <label>
      Rated
      <select aria-label="Rated" bind:value={form.rated}>
        <option value="">Either</option>
        <option value="rated">Rated</option>
        <option value="unrated">Unrated</option>
      </select>
    </label>
    {#if filtered}<button type="button" onclick={() => (form = { ...EMPTY_FORM })}>Clear</button>{/if}
  </form>

  {#if error}<p class="error" role="alert">{error}</p>{/if}
  {#if !available}
    <p class="empty">There's no game database: it couldn't be opened.</p>
  {:else if total === 0 && !loading}
    <p class="empty">{filtered ? 'No game matches.' : 'No games yet.'}</p>
  {:else}
    <ul class="list games" aria-label="Games">
      {#each games as g (g.id)}
        <li class="pick">
          <button
            class="open"
            onclick={() => open(g.id)}
            disabled={opening != null}
            aria-label="Open {g.gold ?? '?'} vs {g.silver ?? '?'}, {g.date}"
          >
            <span class="row-text">
              <span class="row-line">
                <Matchup gold={g.gold} silver={g.silver} goldRating={g.goldRating} silverRating={g.silverRating} />
                {#if g.event && !/^casual game$/i.test(g.event)}<span class="event">{g.event}</span>{/if}
              </span>
              <span class="meta">{libraryMeta(g)}</span>
            </span>
            <span class="action">{opening === g.id ? 'Opening…' : 'Open'}</span>
          </button>
          <button
            class="delete bare"
            class:confirm={confirming === g.id}
            onclick={() => remove(g.id)}
            onblur={() => confirming === g.id && (confirming = null)}
            title={confirming === g.id ? 'Click again to delete it' : 'Delete this game'}
            aria-label={confirming === g.id
              ? 'Confirm deleting this game'
              : `Delete ${g.gold ?? '?'} vs ${g.silver ?? '?'}, ${g.date}`}
            >{#if confirming === g.id}Delete?{:else}<Icon name="close" size={14} />{/if}</button
          >
        </li>
      {/each}
    </ul>
    {#if total != null && games.length < total}
      <button class="more" onclick={() => load(true)} disabled={loading}>
        {loading ? 'Loading…' : `Show more (${(total - games.length).toLocaleString()} left)`}
      </button>
    {:else if loading && games.length === 0}
      <p class="empty">Loading…</p>
    {/if}
  {/if}
</div>

<style>
  header {
    display: flex;
    align-items: baseline;
    gap: 10px;
    margin-bottom: 6px;
    min-height: 30px;
  }
  h1 {
    margin: 0;
    font-size: 20px;
  }
  header button.icon {
    padding: 4px 6px;
    align-self: center;
  }
  .count {
    color: var(--muted);
  }
  .intro {
    margin-bottom: 12px;
  }
  .filters {
    display: flex;
    flex-wrap: wrap;
    align-items: end;
    gap: 8px 12px;
    margin-bottom: 14px;
  }
  .filters label {
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: 12px;
    color: var(--muted);
  }
  .filters input:not([type='date']) {
    width: 10em;
  }
  .games > li.pick {
    display: flex;
    align-items: stretch;
  }
  .games > li.pick > button.open {
    flex: 1;
    width: auto;
    min-width: 0;
  }
  .games > li.pick > button.delete {
    flex: none;
    width: auto;
    padding: 0 12px;
    color: var(--muted);
    background: none;
  }
  .event {
    margin-left: 8px;
    color: var(--muted);
  }
  .delete:hover,
  .delete:focus-visible {
    color: var(--warn);
  }
  .delete.confirm {
    color: var(--warn);
    font-weight: 600;
  }
  .base {
    margin-bottom: 14px;
    padding: 12px 14px;
  }
  .base p {
    margin: 0 0 8px;
  }
  .base .size {
    color: var(--muted);
  }
  .progress {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .progress progress {
    width: 16em;
  }
  .done {
    margin-bottom: 12px;
  }
  .more {
    margin-top: 10px;
  }
  .error {
    margin-bottom: 10px;
  }
</style>
