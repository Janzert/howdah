<script lang="ts">
  // One engine test, as a panel of its own in the lobby window (listed
  // under Engine tests in the sidebar): its results (an SPSA run's in
  // `SpsaPanel`, a pool's in `PoolPanel`), its running and
  // finished games (each opens on a board: a running one followed live), and pausing,
  // resuming, stopping, and its settings (`TestSettingsDialog`). Once it's not
  // active and has no games running, it can be archived (zipped into the
  // daemon's archive/) or deleted.
  import { onMount } from 'svelte';
  import { api, errorMessage } from './api';
  import type { TestGameView } from './bindings/TestGameView';
  import type { TestView } from './bindings/TestView';
  import { on } from './events';
  import PoolPanel from './PoolPanel.svelte';
  import SpsaPanel from './SpsaPanel.svelte';
  import SprtFunnel from './SprtFunnel.svelte';
  import TestSettingsDialog from './TestSettingsDialog.svelte';
  import {
    cutoffPercent,
    decideText,
    endingText,
    endsRows,
    endsText,
    endsTimeText,
    gamesPerHour,
    llrPosition,
    oddsText,
    scoreText,
    sprtText,
    testState,
  } from './tests';

  interface Props {
    id: string;
    onOpenGame: (test: string, pair: number, game: number) => Promise<string | null>;
    /** Opens a game window that follows the test from game to game. */
    onFollow: (test: string) => Promise<string | null>;
    /** The test was archived (to `archive`) or deleted. */
    onRemoved: (archive: string | null) => void;
    /** Closes the panel. */
    onClose: () => void;
  }
  let { id, onOpenGame, onFollow, onRemoved, onClose }: Props = $props();

  /** Finished games listed at first, and added by each Show more. */
  const GAMES_PAGE = 100;

  let test = $state<TestView | null>(null);
  let games = $state<TestGameView[]>([]);
  let error = $state<string | null>(null);
  let busy = $state(false);
  let settingsOpen = $state(false);
  /** Delete was clicked once; the second click deletes. */
  let confirmDelete = $state(false);
  /** Finished games listed; Show more adds to it. */
  let gamesShown = $state(GAMES_PAGE);
  /** The SPRT funnel shown full size over the panel. */
  let funnelOpen = $state(false);
  let funnelDialog = $state<HTMLDialogElement>();
  $effect(() => {
    if (funnelOpen) funnelDialog?.showModal();
  });

  function show(t: TestView) {
    test = t;
  }

  onMount(() => {
    api
      .tourneyTest(id)
      .then((d) => {
        show(d.test);
        games = d.games;
      })
      .catch((e) => (error = errorMessage(e)));
    const unlisten = on('tourney://changed', (u) => {
      if (u.test?.id === id) test = u.test;
      // A game that finished while the panel loaded can come in both ways.
      const g = u.game;
      if (g?.test === id && !games.some((o) => o.pair === g.pair && o.game === g.game)) games = [g, ...games];
    });
    return () => void unlisten.then((f) => f());
  });

  async function act(f: () => Promise<TestView>) {
    busy = true;
    error = null;
    try {
      show(await f());
    } catch (e) {
      error = errorMessage(e);
    }
    busy = false;
  }

  async function remove(archive: boolean) {
    if (!archive && !confirmDelete) {
      confirmDelete = true;
      return;
    }
    confirmDelete = false;
    busy = true;
    error = null;
    try {
      const zip = await api.tourneyRemove(id, archive);
      onRemoved(zip);
    } catch (e) {
      error = errorMessage(e);
    }
    busy = false;
  }

  async function open(pair: number, game: number) {
    error = await onOpenGame(id, pair, game);
  }

  async function follow() {
    error = await onFollow(id);
  }

  const pos = $derived(test ? llrPosition(test) : null);
  const perHour = $derived(gamesPerHour(games.map((g) => g.finished)));
  const ends = $derived(test ? endsRows(test) : null);
  const removable = $derived(test != null && test.status !== 'active' && test.running.length === 0);
  /** The settings in brief, for beside the Settings button. */
  const settingsText = $derived.by(() => {
    if (!test) return '';
    const parts = test.spsa
      ? [`${test.spsa.iterations} iterations of ${test.spsa.pairsPerIteration} pairs`]
      : [test.maxPairs != null ? `max ${test.maxPairs} pairs` : test.pool ? 'until stopped' : 'no pair limit'];
    const cutoff = cutoffPercent(test);
    if (test.sprt && test.maxPairs != null && cutoff != null) parts.push(`undecided under ${cutoff}%`);
    parts.push(`${test.concurrency} at once`, `priority ${test.priority}`);
    return parts.join(' · ');
  });
  const won = (g: TestGameView) => (g.winner === 'gold' ? g.gold : g.silver);
</script>

<div class="screen">
  <header>
    <h1 id="test-title">{test?.name ?? 'Engine test'}</h1>
    {#if test}<span class="state">{testState(test)}</span>{/if}
    <span class="spacer"></span>
    <button onclick={onClose} title="Close this panel (the test goes on)">Close</button>
  </header>
  <div class="card" class:wide={!!test?.spsa || !!test?.pool}>
    {#if test}
      <div class="title-row">
        <p class="note">
          {#if test.spsa}
            SPSA tuning: <strong>{test.engines[0]}</strong> (θ + cΔ) vs {test.engines[1]} (θ − cΔ)
          {:else if test.pool}
            A pool of {test.pool.engines.length} engines, {test.games} games
          {:else}
            <strong>{test.engines[0]}</strong> (tested) vs {test.engines[1]}
          {/if}{test.timeControl ? `, ${test.timeControl}` : ', no clock'}
        </p>
        <span class="spacer"></span>
        {#if test.status === 'active'}
          <button
            onclick={() => act(() => api.tourneyControl(id, 'pause'))}
            disabled={busy}
            title="Start no more games until resumed; the running ones finish">Pause</button
          >
        {:else if test.outcome !== 'passed' && test.outcome !== 'failed'}
          <button onclick={() => act(() => api.tourneyControl(id, 'resume'))} disabled={busy}>Resume</button>
        {/if}
        {#if test.status !== 'finished'}
          <button
            onclick={() => act(() => api.tourneyControl(id, 'stop'))}
            disabled={busy}
            title="End the test; its running games finish">Stop</button
          >
        {/if}
      </div>
      {#if test.halted}<p class="error">Halted: {test.halted}</p>{/if}

      {#if test.spsa}
        <SpsaPanel test={{ ...test, spsa: test.spsa }} />
      {:else if test.pool}
        <PoolPanel test={{ ...test, pool: test.pool }} onChanged={show} />
      {:else}
        <div class="results">
          <dl class="stats">
            <dt>Score</dt>
            <dd>{scoreText(test)}{test.score != null ? `, ${(test.score * 100).toFixed(1)}%` : ''}</dd>
            <dt>Pairs</dt>
            <dd>
              {test.engines[0]} won both {test.pairCounts[2]}, split {test.pairCounts[1]}, lost both {test
                .pairCounts[0]}
            </dd>
            {#if test.elo}
              <dt>Elo</dt>
              <dd>{test.elo}{test.los != null ? `, LOS ${(test.los * 100).toFixed(1)}%` : ''}</dd>
            {/if}
            {#if sprtText(test)}
              <dt>SPRT</dt>
              <dd>{sprtText(test)}</dd>
            {/if}
            {#if ends}
              <dt>Ends</dt>
              <dd
                title={test.sprt
                  ? "Simulated from the results so far, with the test's own pass, fail and undecided rules: the median games left, and the range holding 80% of the endings; apart for passing or failing and not, when both are likely"
                  : 'The games left to play every pair'}
              >
                {#each ends as row (row.label)}
                  {@const games = endsText(test, row.gamesLeft)}
                  {@const time = endsTimeText(test, perHour, row.gamesLeft)}
                  <div>
                    {row.label ? `${row.label}: ${games}` : games}{#if time}<span
                        title="At the pace of the games since the last pause, {perHour?.toFixed(1)} an hour"
                        >, {time}</span
                      >{/if}
                  </div>
                {/each}
              </dd>
            {/if}
            {#if oddsText(test) || decideText(test)}
              <dt>Outcome</dt>
              <dd>
                {#if oddsText(test)}<span title="How often each ending came up in the simulations"
                    >{oddsText(test)}</span
                  >{/if}
                {#if decideText(test)}
                  <span
                    class="aside"
                    title="The chance the LLR reaches a bound before the pair limit, which the undecided cutoff watches"
                    >{decideText(test)}</span
                  >
                {/if}
              </dd>
            {/if}
          </dl>
          {#if test.sprt}
            <button
              class="funnel-button"
              aria-label="SPRT funnel, full size"
              onclick={() => (funnelOpen = true)}
              title="The SPRT funnel: the observed Elo after each game, between the Elo at which the test fails (left) and passes (right). Click for full size"
            >
              <SprtFunnel {test} {games} mini />
            </button>
          {/if}
        </div>
        {#if pos != null}<span class="llr"><span class="mark" style="left: {pos * 100}%"></span></span>{/if}
      {/if}
      {#if test.repeated > 0}
        <p class="error">
          {test.repeated} games repeat an earlier game's moves, so the error bars are narrower than they should be.
        </p>
      {/if}

      <div class="row">
        <span class="settings">{settingsText}</span>
        <button
          class="subtle"
          onclick={() => (settingsOpen = true)}
          title={test.spsa
            ? 'Change the games at once and priority'
            : 'Change the pair limit, games at once and priority'}>Settings…</button
        >
      </div>
      {#if error}<p class="error" role="alert">{error}</p>{/if}

      {#if test.running.length > 0}
        <div class="heading">
          <h3>Playing now</h3>
          <button
            class="subtle"
            onclick={follow}
            title="Watch the newest game without a window, and as each game ends, go on to the next one">Follow</button
          >
        </div>
        <ul class="games" aria-label="Games playing now">
          {#each test.running as r (`${r.pair}.${r.game}`)}
            <li>
              <span class="num">{r.pair}.{r.game}</span>
              <span>{r.gold} (gold) vs {r.silver}</span>
              <button class="subtle" onclick={() => open(r.pair, r.game)}>Watch</button>
            </li>
          {/each}
        </ul>
      {/if}

      <h3>Games</h3>
      {#if games.length === 0}
        <p class="note">None finished yet.</p>
      {:else}
        <ul class="games list" aria-label="Finished games">
          {#each games.slice(0, gamesShown) as g (`${g.pair}.${g.game}`)}
            <li>
              <span class="num">{g.pair}.{g.game}</span>
              <span class="who">{g.gold} vs {g.silver}</span>
              <span class="result">{won(g)} ({g.winner}) by {endingText(g.reason)}, {g.plies} plies</span>
              <button class="subtle" onclick={() => open(g.pair, g.game)}>Open</button>
            </li>
          {/each}
        </ul>
        {#if games.length > gamesShown}
          <button class="subtle more" onclick={() => (gamesShown += GAMES_PAGE)}
            >Show more ({games.length - gamesShown} older)</button
          >
        {/if}
      {/if}
      <p class="note path">{test.path}</p>
      {#if funnelOpen}
        <dialog
          class="funnel-dialog"
          bind:this={funnelDialog}
          onclose={() => (funnelOpen = false)}
          onclick={(e) => e.target === funnelDialog && funnelDialog.close()}
          aria-labelledby="funnel-title"
        >
          <div class="heading">
            <h2 id="funnel-title">SPRT funnel</h2>
            <span class="note">{sprtText(test)}</span>
            <span class="spacer"></span>
            <button onclick={() => funnelDialog?.close()}>Close</button>
          </div>
          <SprtFunnel {test} {games} height={Math.max(320, Math.min(720, window.innerHeight - 200))} />
        </dialog>
      {/if}
      {#if settingsOpen}
        <TestSettingsDialog {test} onSaved={show} onClose={() => (settingsOpen = false)} />
      {/if}
    {:else if error}
      <p class="error" role="alert">{error}</p>
    {:else}
      <p>Loading…</p>
    {/if}
    <div class="buttons">
      {#if test}
        <button
          onclick={() => remove(true)}
          disabled={busy || !removable}
          title={removable
            ? "Zip the test's directory into the daemon's archive folder and take it off the list"
            : 'Stop or pause the test, and let its games finish, first'}>Archive</button
        >
        <button
          class:danger={confirmDelete}
          onclick={() => remove(false)}
          onblur={() => (confirmDelete = false)}
          disabled={busy || !removable}
          title={removable
            ? 'Delete the test and all its games, without keeping a copy'
            : 'Stop or pause the test, and let its games finish, first'}
          >{confirmDelete ? 'Delete all its games?' : 'Delete'}</button
        >
      {/if}
    </div>
  </div>
</div>

<style>
  header {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 14px;
    min-height: 30px;
  }
  h1 {
    margin: 0;
    font-size: 20px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .card {
    max-width: 820px;
  }
  .card.wide {
    max-width: 1100px;
  }
  .more {
    margin-top: 4px;
  }
  h3 {
    margin: 12px 0 4px;
    font-size: 14px;
  }
  .heading {
    display: flex;
    align-items: baseline;
    gap: 8px;
  }
  .state {
    font-size: 11px;
    font-weight: 400;
    padding: 0 6px;
    border-radius: 8px;
    border: 1px solid var(--border);
    color: var(--muted);
  }
  .aside {
    display: block;
    font-size: 12px;
    color: var(--muted);
  }
  .note {
    margin: 0 0 8px;
    font-size: 12px;
    color: var(--muted);
  }
  .path {
    margin-top: 8px;
    overflow-wrap: anywhere;
  }
  .results {
    display: flex;
    align-items: flex-start;
    gap: 12px;
  }
  .results .stats {
    flex: 1;
    min-width: 0;
  }
  .funnel-button {
    flex: none;
    margin-top: 6px;
    padding: 3px;
    background: none;
    border: 1px solid var(--border);
    border-radius: 6px;
    cursor: zoom-in;
  }
  .funnel-button:hover {
    background: var(--hover);
  }
  .funnel-dialog {
    width: min(1000px, 94vw);
  }
  .funnel-dialog h2 {
    margin: 0;
    font-size: 18px;
  }
  .funnel-dialog .heading {
    align-items: center;
    margin-bottom: 8px;
  }
  .funnel-dialog .note {
    margin: 0;
  }
  .stats {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 2px 12px;
    margin: 6px 0;
    font-size: 13px;
  }
  dt {
    color: var(--muted);
  }
  dd {
    margin: 0;
  }
  .llr {
    position: relative;
    display: block;
    height: 4px;
    margin: 8px 0 10px;
    border-radius: 2px;
    background: linear-gradient(to right, var(--warn), var(--border) 50%, var(--clock-ok));
  }
  .mark {
    position: absolute;
    top: -3px;
    width: 3px;
    height: 10px;
    margin-left: -1px;
    background: var(--text);
  }
  .row {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
    font-size: 13px;
  }
  .title-row {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
    margin-bottom: 8px;
  }
  .title-row .note {
    margin: 0;
  }
  .settings {
    color: var(--muted);
  }
  .games {
    list-style: none;
    margin: 0;
    padding: 0;
    font-size: 13px;
  }
  .games.list {
    overflow-y: auto;
    max-height: 50vh;
  }
  .games li {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 2px 0;
  }
  .num {
    flex: none;
    width: 4em;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }
  .who {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .result {
    color: var(--muted);
  }
  .error {
    color: var(--warn);
    font-size: 13px;
  }
  .buttons {
    display: flex;
    gap: 6px;
    margin-top: 14px;
  }
  .spacer {
    flex: 1;
  }
  .subtle {
    font-size: 12px;
    padding: 2px 8px;
    color: var(--muted);
  }
  .danger {
    color: var(--warn);
  }
</style>
