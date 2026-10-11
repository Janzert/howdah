<script lang="ts">
  // One engine test: its results, its running and finished games (each
  // opens in a game window: a running one followed live), and pausing,
  // resuming, stopping, its priority and its pair limit.
  import { onMount } from 'svelte';
  import { api, errorMessage } from './api';
  import type { TestGameView } from './bindings/TestGameView';
  import type { TestView } from './bindings/TestView';
  import { on } from './events';
  import { llrPosition, pairCount, scoreText, sprtText, testState } from './tests';

  interface Props {
    id: string;
    onOpenGame: (test: string, pair: number, game: number) => Promise<string | null>;
    onClose: () => void;
  }
  let { id, onOpenGame, onClose }: Props = $props();

  let test = $state<TestView | null>(null);
  let games = $state<TestGameView[]>([]);
  let error = $state<string | null>(null);
  let busy = $state(false);
  let priority = $state(0);
  let maxPairs = $state<number | null>(null);
  let dialog: HTMLDialogElement;

  $effect(() => {
    dialog.showModal();
  });

  function show(t: TestView) {
    test = t;
    priority = t.priority;
    maxPairs = t.maxPairs;
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

  function applyLimits() {
    void act(async () => {
      let t = test!;
      if (priority !== t.priority) t = await api.tourneySetPriority(id, priority);
      const pairs = pairCount(maxPairs);
      if (pairs !== t.maxPairs) t = await api.tourneySetMaxPairs(id, pairs);
      return t;
    });
  }

  async function open(pair: number, game: number) {
    error = await onOpenGame(id, pair, game);
  }

  const pos = $derived(test ? llrPosition(test) : null);
  const won = (g: TestGameView) => (g.winner === 'gold' ? g.gold : g.silver);
</script>

<dialog bind:this={dialog} onclose={onClose} aria-labelledby="test-title">
  {#if test}
    <h2 id="test-title">
      {test.name}
      <span class="state">{testState(test)}</span>
    </h2>
    <p class="note">
      <strong>{test.engines[0]}</strong> (tested) vs {test.engines[1]}{test.timeControl
        ? `, ${test.timeControl}`
        : ', no clock'}
    </p>
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
    </dl>
    {#if test.repeated > 0}
      <p class="error">
        {test.repeated} games repeat an earlier game's moves, so the error bars are narrower than they should be.
      </p>
    {/if}

    <div class="row">
      {#if test.status === 'active'}
        <button onclick={() => act(() => api.tourneyControl(id, 'pause'))} disabled={busy}>Pause</button>
      {:else if test.outcome !== 'passed' && test.outcome !== 'failed'}
        <button onclick={() => act(() => api.tourneyControl(id, 'resume'))} disabled={busy}>Resume</button>
      {/if}
      {#if test.status !== 'finished'}
        <button onclick={() => act(() => api.tourneyControl(id, 'stop'))} disabled={busy}
          title="End the test; its running games finish">Stop</button>
      {/if}
      <span class="spacer"></span>
      <label for="test-priority">Priority</label>
      <input id="test-priority" type="number" bind:value={priority} />
      <label for="test-pairs">Max pairs</label>
      <input id="test-pairs" type="number" min="1" bind:value={maxPairs} placeholder="none" />
      <button onclick={applyLimits} disabled={busy}>Apply</button>
    </div>
    {#if error}<p class="error" role="alert">{error}</p>{/if}

    {#if test.running.length > 0}
      <h3>Playing now</h3>
      <ul class="games" aria-label="Games playing now">
        {#each test.running as r (`${r.pair}.${r.game}`)}
          <li>
            <span class="num">{r.pair}.{r.game}</span>
            <span>{r.game === 1 ? test.engines[0] : test.engines[1]} (gold) vs {r.game === 1
                ? test.engines[1]
                : test.engines[0]}</span>
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
            <span class="result">{won(g)} ({g.winner}) by {g.reason}, {g.plies} plies</span>
            <button class="subtle" onclick={() => open(g.pair, g.game)}>Open</button>
          </li>
        {/each}
      </ul>
    {/if}
    <p class="note path">{test.path}</p>
  {:else if error}
    <h2 id="test-title">Engine test</h2>
    <p class="error" role="alert">{error}</p>
  {:else}
    <h2 id="test-title">Engine test</h2>
    <p>Loading…</p>
  {/if}
  <div class="buttons">
    <span class="spacer"></span>
    <button onclick={() => dialog.close()}>Close</button>
  </div>
</dialog>

<style>
  dialog {
    width: min(680px, 92vw);
    max-height: 88vh;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--panel);
    color: var(--text);
    padding: 16px;
  }
  dialog[open] {
    display: flex;
    flex-direction: column;
  }
  dialog::backdrop {
    background: rgba(0, 0, 0, 0.4);
  }
  h2 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 0 4px;
    font-size: 16px;
  }
  h3 {
    margin: 12px 0 4px;
    font-size: 14px;
  }
  .state {
    font-size: 11px;
    font-weight: 400;
    padding: 0 6px;
    border-radius: 8px;
    border: 1px solid var(--border);
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
  .row input[type='number'] {
    width: 5em;
  }
  .games {
    list-style: none;
    margin: 0;
    padding: 0;
    font-size: 13px;
  }
  .games.list {
    overflow-y: auto;
    min-height: 0;
    max-height: 40vh;
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
</style>
