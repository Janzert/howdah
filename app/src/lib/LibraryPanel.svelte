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
  import type { LibraryImportView } from './bindings/LibraryImportView';
  import Icon from './Icon.svelte';
  import Matchup from './gameroom/Matchup.svelte';
  import SetupDiagram from './SetupDiagram.svelte';
  import SetupExplorer from './SetupExplorer.svelte';
  import { EMPTY_FORM, ENDINGS, importWords, libraryMeta, sourceName, toFilter, type FilterForm } from './library';
  import type { LibrarySourceView } from './bindings/LibrarySourceView';
  import ExportDialog from './ExportDialog.svelte';
  import SourcesDialog from './SourcesDialog.svelte';
  import { IMPORT_FILTERS, chooseOpenPath, nativeFiles } from './files';
  import { pref, savePref } from './gameroom/store.svelte';
  import PositionEditor from './editor/PositionEditor.svelte';
  import { EditorBoard } from './editor/editorBoard';
  import MiniBoard from '@howdah/board/board/MiniBoard.svelte';
  import { findTheme } from '@howdah/board/theme';
  import { settings } from './settings.svelte';
  import type { PositionSpec } from './bindings/PositionSpec';

  interface Props {
    /** Opens a game, at the position after `ply` moves if given; resolves
     * to an error message, or null. */
    onOpen: (id: number, ply: number | null) => Promise<string | null>;
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
  /** The list of games, or the setup explorer. */
  let view = $state<'games' | 'setups'>(pref('lobby.gamesView') === 'setups' ? 'setups' : 'games');
  $effect(() => savePref('lobby.gamesView', view));
  /** The filter without the setups, for the explorer. */
  // A position from the setup explorer's line (matched with its mirror
  // image) is left out too: it came from there.
  const explorerFilter = $derived(
    toFilter({
      ...form,
      goldSetup: '',
      silverSetup: '',
      position: form.positionMatch === 'mirrored' ? '' : form.position,
    }),
  );

  function showSetupGames(gold: string | null, silver: string | null) {
    form = { ...form, goldSetup: gold ?? '', silverSetup: silver ?? '' };
    view = 'games';
  }
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

  async function open(id: number, ply: number | null) {
    opening = id;
    error = await onOpen(id, ply);
    opening = null;
  }

  // The position filter, set in the position editor shown over the panel.
  const theme = $derived(findTheme(settings.theme));
  let editing = $state<PositionSpec | null>(null);
  let editorFlipped = $state(false);
  async function editPosition() {
    editing = form.position
      ? { short: form.position, sideToMove: form.positionSide, moveNumber: 2 }
      : await api.defaultPosition();
  }
  function searchPosition(p: PositionSpec, match: 'exact' | 'pieces') {
    form = { ...form, position: p.short, positionSide: p.sideToMove, positionMatch: match };
    editing = null;
    view = 'games';
  }
  const positionPreview = $derived(
    form.position
      ? {
          sideToMove: form.positionSide,
          pieces: EditorBoard.fromShort(form.position).pieceViews(),
          short: form.position,
        }
      : null,
  );

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

  // Sharing: Export… writes the list's games to a file, Import… takes a
  // game database file or a record file in, and Sources… lists where
  // everything came from. The Source filter offers the sources too.
  let exporting = $state(false);
  let showSources = $state(false);
  let sources = $state<LibrarySourceView[]>([]);
  /** In the browser preview, the path being typed for an import. */
  let importPath = $state<string | null>(null);
  let importing = $state(false);
  let imported = $state<string | null>(null);
  let importError = $state<string | null>(null);

  async function loadSources() {
    try {
      sources = await api.librarySources();
    } catch {
      sources = [];
    }
  }
  void loadSources();
  const otherSources = $derived(sources.filter((s) => !s.own && s.games > 0));

  /** The Source select's value: `arimaa`, `local`, or `db:<id>`. */
  const sourceValue = $derived(form.fromDb ? `db:${form.fromDb}` : form.origin);
  function setSource(v: string) {
    form = v.startsWith('db:')
      ? { ...form, origin: '', fromDb: v.slice(3) }
      : { ...form, origin: v as FilterForm['origin'], fromDb: '' };
  }

  async function startImport() {
    imported = null;
    importError = null;
    if (!nativeFiles) {
      importPath = importPath ?? '';
      return;
    }
    const path = await chooseOpenPath(IMPORT_FILTERS);
    if (path) await importFrom(path);
  }

  /** An import that stopped at earlier lines from the file's source (an
   * earlier analysis by the same engine), waiting for the user's choice. */
  let askEarlier = $state<{ path: string; view: LibraryImportView } | null>(null);

  async function importFrom(path: string, earlier?: 'replace' | 'keep') {
    if (!path.trim()) return;
    importing = true;
    importError = null;
    askEarlier = null;
    try {
      const view = await api.libraryImport(path.trim(), earlier);
      if (view.undecided) {
        askEarlier = { path: path.trim(), view };
      } else {
        imported = importWords(view);
        importPath = null;
        void load();
        void loadSources();
      }
    } catch (e) {
      importError = errorMessage(e);
    }
    importing = false;
  }

  /** What the list shows, for an export's file name. */
  const exportWhat = $derived(
    [form.player.trim(), form.opponent.trim() && `vs ${form.opponent.trim()}`].filter((x) => x).join(' ') ||
      (JSON.stringify(form) !== JSON.stringify(EMPTY_FORM) ? 'selection' : 'all'),
  );

  // The base database (arimaa.com's archive): offered when there's a newer
  // one than the database has, with its download's progress.
  let base = $state<BaseStatus | null>(null);
  let progress = $state<BaseProgress | null>(null);
  let baseError = $state<string | null>(null);
  let baseDone = $state<string | null>(null);

  /** `check` asks for the manifest now, not from the day's check. */
  async function loadBase(check = false) {
    try {
      base = await api.baseStatus(check);
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

{#if exporting}
  <ExportDialog filter={toFilter(form)} count={total} what={exportWhat} onClose={() => (exporting = false)} />
{/if}
{#if showSources}
  <SourcesDialog
    onShow={(id) => {
      form = { ...form, origin: '', fromDb: String(id) };
      view = 'games';
    }}
    onChanged={() => {
      void load();
      void loadSources();
    }}
    onClose={() => (showSources = false)}
  />
{/if}

{#if editing}
  <div class="editor-over">
    <PositionEditor
      initial={editing}
      {theme}
      bind:flipped={editorFlipped}
      onSearch={searchPosition}
      onClose={() => (editing = null)}
    />
  </div>
{/if}

<div class="screen">
  <header>
    <h1>Games</h1>
    <div class="tabs views" role="tablist" aria-label="View">
      <button role="tab" aria-selected={view === 'games'} class:on={view === 'games'} onclick={() => (view = 'games')}
        >List</button
      >
      <button
        role="tab"
        aria-selected={view === 'setups'}
        class:on={view === 'setups'}
        onclick={() => (view = 'setups')}>Setups</button
      >
    </div>
    {#if view === 'games'}<span class="count" aria-live="polite">{countText}</span>{/if}
    <span class="spacer"></span>
    <button onclick={startImport} disabled={importing} title="Add the games from a game database file or a record file"
      >{importing ? 'Importing…' : 'Import…'}</button
    >
    <button onclick={() => (exporting = true)} disabled={!available} title="Write the list's games to a file to share"
      >Export…</button
    >
    {#if otherSources.length > 0}
      <button onclick={() => (showSources = true)} title="Where your games came from">Sources…</button>
    {/if}
    <button
      class="icon"
      onclick={() => {
        void load();
        void loadBase(true);
        void loadSources();
      }}
      title="Load the list again and check for new games from arimaa.com's archive"
      aria-label="Refresh"><Icon name="refresh" size={16} /></button
    >
  </header>
  <p class="hint intro">
    Every game you play, watch, analyse or open is kept here, with arimaa.com's games you've opened. Each opens on a
    board of its own, and what you add to it is saved with it.
  </p>

  {#if importPath != null}
    <form
      class="import-path"
      aria-label="Import a file"
      onsubmit={(e) => {
        e.preventDefault();
        void importFrom(importPath ?? '');
      }}
    >
      <label
        >File to import (a path on this computer)
        <input bind:value={importPath} aria-label="File to import" autocomplete="off" spellcheck="false" /></label
      >
      <button class="primary" disabled={importing || !importPath.trim()}>{importing ? 'Importing…' : 'Import'}</button>
      <button type="button" onclick={() => (importPath = null)}>Cancel</button>
    </form>
  {/if}
  {#if askEarlier}
    {@const games = askEarlier.view.earlier}
    {@const sources = [...new Set(games.map((g) => g.source))].join(', ')}
    <section class="card earlier" role="alert" aria-label="Earlier lines">
      <p>
        {askEarlier.view.name} has a newer analysis by {sources} of {games.length === 1
          ? 'a game'
          : `${games.length} games`} you already have, whose lines differ from the earlier one's:
      </p>
      <ul>
        {#each games as g (g.game)}
          <li>{g.title}: {g.moves} {g.moves === 1 ? 'move' : 'moves'} of earlier lines</li>
        {/each}
      </ul>
      <p class="hint">
        Replacing removes the earlier lines, except any you've gone on from or commented on. Either way the moves'
        comments become the newer ones, unless you've edited them.
      </p>
      <p class="actions">
        <button class="primary" disabled={importing} onclick={() => importFrom(askEarlier!.path, 'replace')}
          >Replace earlier lines</button
        >
        <button disabled={importing} onclick={() => importFrom(askEarlier!.path, 'keep')}>Keep both</button>
        <button disabled={importing} onclick={() => (askEarlier = null)}>Cancel</button>
      </p>
    </section>
  {/if}
  {#if imported}<p class="hint done" role="status">Imported {imported}</p>{/if}
  {#if importError}<p class="error" role="alert">{importError}</p>{/if}

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
      <select aria-label="Source" value={sourceValue} onchange={(e) => setSource(e.currentTarget.value)}>
        <option value="">Anywhere</option>
        <option value="arimaa">arimaa.com</option>
        <option value="local">This app</option>
        {#each otherSources as s (s.id)}
          {#if s.name !== 'base'}<option value={`db:${s.id}`}>From {sourceName(s)}</option>{/if}
        {/each}
        {#if form.fromDb && !otherSources.some((s) => String(s.id) === form.fromDb)}
          <option value={`db:${form.fromDb}`}>From a source</option>
        {/if}
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

  {#if view === 'games' && (form.goldSetup || form.silverSetup)}
    <div class="setup-filter" aria-label="Setups the list is narrowed to" role="group">
      {#if form.goldSetup}
        <span class="chip">Gold <SetupDiagram setup={form.goldSetup} color="gold" size={14} /></span>
      {/if}
      {#if form.silverSetup}
        <span class="chip"
          >{form.goldSetup ? "Silver's reply" : 'Silver'}
          <SetupDiagram setup={form.silverSetup} color="silver" size={14} /></span
        >
      {/if}
      <button type="button" onclick={() => (view = 'setups')}>Setups…</button>
      <button type="button" onclick={() => (form = { ...form, goldSetup: '', silverSetup: '' })}>Any setups</button>
    </div>
  {/if}

  <div class="position-filter" role="group" aria-label="Position the list is narrowed to">
    {#if positionPreview}
      <span class="mini" title={form.positionMatch === 'pieces' ? 'These pieces on these squares' : 'This position'}
        ><MiniBoard position={positionPreview} {theme} flipped={false} /></span
      >
      <label>
        <select aria-label="Position match" bind:value={form.positionMatch}>
          <option value="exact">This position, {form.positionSide} to move</option>
          <option value="pieces">These pieces, anything else anywhere</option>
          <option value="mirrored">This position or its mirror image</option>
        </select>
      </label>
      <button type="button" onclick={editPosition}>Edit position…</button>
      <button type="button" onclick={() => (form = { ...form, position: '' })}>Any position</button>
    {:else}
      <button
        type="button"
        onclick={editPosition}
        title="Find the games reaching a position, or with some pieces on some squares">Position…</button
      >
    {/if}
  </div>

  {#if view === 'setups'}
    <SetupExplorer
      filter={explorerFilter}
      onGames={showSetupGames}
      onPosition={(p) => {
        form = {
          ...form,
          goldSetup: '',
          silverSetup: '',
          position: p.short,
          positionSide: p.sideToMove,
          positionMatch: 'mirrored',
        };
        view = 'games';
      }}
    />
  {:else}
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
              onclick={() => open(g.id, g.positionPly)}
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
  {/if}
</div>

<style>
  .editor-over {
    position: fixed;
    inset: 0;
    z-index: 50;
    background: var(--bg);
  }
  .position-filter {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 12px;
    margin: -6px 0 12px;
  }
  .position-filter .mini {
    width: 72px;
    height: 72px;
    display: block;
  }
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
  .views {
    display: flex;
    gap: 2px;
    align-self: center;
  }
  .views button {
    background: none;
    border: none;
    border-bottom: 2px solid transparent;
    border-radius: 0;
    padding: 2px 8px;
    font-weight: 600;
    color: var(--muted);
  }
  .views button.on {
    color: var(--text);
    border-bottom-color: var(--accent);
  }
  .setup-filter {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 12px;
    margin: -6px 0 12px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
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
  .earlier p {
    margin: 0 0 0.4rem;
  }
  .earlier ul {
    margin: 0 0 0.4rem;
    padding-left: 1.2rem;
  }
  .earlier .actions {
    display: flex;
    gap: 0.5rem;
    flex-wrap: wrap;
    margin: 0;
  }
  .import-path {
    display: flex;
    flex-wrap: wrap;
    align-items: end;
    gap: 8px;
    margin-bottom: 12px;
  }
  .import-path label {
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: 12px;
    color: var(--muted);
    flex: 1;
    min-width: 16em;
  }
  .error {
    margin-bottom: 10px;
  }
</style>
