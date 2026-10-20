<script lang="ts">
  // One engine test, as a panel of its own in the lobby window (listed
  // under Engine tests in the sidebar): its results, its running and
  // finished games (each opens on a board: a running one followed live), and pausing,
  // resuming, stopping, and its settings (`TestSettingsDialog`). Once it's not
  // active and has no games running, it can be archived (zipped into the
  // daemon's archive/) or deleted.
  import { onMount } from 'svelte';
  import { api, errorMessage } from './api';
  import type { TestGameView } from './bindings/TestGameView';
  import type { TestView } from './bindings/TestView';
  import { on } from './events';
  import TestSettingsDialog from './TestSettingsDialog.svelte';
  import {
    cutoffPercent,
    decideText,
    endingText,
    endsText,
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

  let test = $state<TestView | null>(null);
  let games = $state<TestGameView[]>([]);
  let error = $state<string | null>(null);
  let busy = $state(false);
  let settingsOpen = $state(false);
  /** Delete was clicked once; the second click deletes. */
  let confirmDelete = $state(false);

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
      if (u.game?.test === id) games = [u.game, ...games];
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
  const removable = $derived(test != null && test.status !== 'active' && test.running.length === 0);
  /** The settings in brief, for beside the Settings button. */
  const settingsText = $derived.by(() => {
    if (!test) return '';
    const parts = [test.maxPairs != null ? `max ${test.maxPairs} pairs` : 'no pair limit'];
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
  <div class="card">
    {#if test}
      <div class="title-row">
        <p class="note">
          <strong>{test.engines[0]}</strong> (tested) vs {test.engines[1]}{test.timeControl
            ? `, ${test.timeControl}`
            : ', no clock'}
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

      <dl class="stats">
        <dt>Score</dt>
        <dd>{scoreText(test)}{test.score != null ? `, ${(test.score * 100).toFixed(1)}%` : ''}</dd>
        <dt>Pairs</dt>
        <dd>
          {test.engines[0]} won both {test.pairCounts[2]}, split {test.pairCounts[1]}, lost both {test.pairCounts[0]}
        </dd>
        {#if test.elo}
          <dt>Elo</dt>
          <dd>{test.elo}{test.los != null ? `, LOS ${(test.los * 100).toFixed(1)}%` : ''}</dd>
        {/if}
        {#if sprtText(test)}
          <dt>SPRT</dt>
          <dd>
            {sprtText(test)}
            {#if pos != null}<span class="llr"><span class="mark" style="left: {pos * 100}%"></span></span>{/if}
          </dd>
        {/if}
        {#if endsText(test)}
          <dt>Ends</dt>
          <dd
            title="Simulated from the results so far, with the test's own pass, fail and undecided rules: the median games left, and the range holding 80% of the endings"
          >
            {endsText(test)}
          </dd>
        {/if}
        {#if oddsText(test) || decideText(test)}
          <dt>Outcome</dt>
          <dd>
            {#if oddsText(test)}<span title="How often each ending came up in the simulations">{oddsText(test)}</span
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
          title="Change the pair limit, games at once and priority">Settings…</button
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
              <span
                >{r.game === 1 ? test.engines[0] : test.engines[1]} (gold) vs {r.game === 1
                  ? test.engines[1]
                  : test.engines[0]}</span
              >
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
          {#each games as g (`${g.pair}.${g.game}`)}
            <li>
              <span class="num">{g.pair}.{g.game}</span>
              <span class="who">{g.gold} vs {g.silver}</span>
              <span class="result">{won(g)} ({g.winner}) by {endingText(g.reason)}, {g.plies} plies</span>
              <button class="subtle" onclick={() => open(g.pair, g.game)}>Open</button>
            </li>
          {/each}
        </ul>
      {/if}
      <p class="note path">{test.path}</p>
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
    margin: 6px 0 2px;
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
