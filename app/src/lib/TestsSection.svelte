<script lang="ts">
  // The lobby's Engine tests: the howdah-tourney daemon's queue of tests
  // (docs/TOURNAMENTS.md). The daemon runs on its own, so tests go on with
  // Howdah closed; this lists them, starts new ones, and opens their games
  // in game windows. Without a daemon it offers to start one.
  import { onMount } from 'svelte';
  import { api, errorMessage } from './api';
  import type { EngineSpec } from './bindings/EngineSpec';
  import type { TestView } from './bindings/TestView';
  import type { TourneyStatusView } from './bindings/TourneyStatusView';
  import { on } from './events';
  import NewTestDialog from './NewTestDialog.svelte';
  import TestDialog from './TestDialog.svelte';
  import { llrPosition, scoreText, testState } from './tests';

  interface Props {
    engines: EngineSpec[];
    /** Opens a test's game in a game window; resolves to an error message,
     * or null. */
    onOpenGame: (test: string, pair: number, game: number) => Promise<string | null>;
  }
  let { engines, onOpenGame }: Props = $props();

  let status = $state<TourneyStatusView | null>(null);
  let tests = $state<TestView[]>([]);
  let error = $state<string | null>(null);
  let busy = $state(false);
  let showNew = $state(false);
  let shown = $state<string | null>(null);
  let startSlots = $state(Math.max(1, Math.floor((navigator.hardwareConcurrency || 2) / 2)));

  async function refresh() {
    try {
      status = await api.tourneyStatus();
      tests = status.connected ? await api.tourneyTests() : [];
      error = null;
    } catch (e) {
      error = errorMessage(e);
    }
  }

  onMount(() => {
    void refresh();
    const unlisten = on('tourney://changed', (u) => {
      if (status && u.connected !== status.connected) {
        void refresh();
        return;
      }
      if (u.test) {
        const t = u.test;
        const i = tests.findIndex((x) => x.id === t.id);
        if (i >= 0) tests[i] = t;
        else tests = [...tests, t];
      }
      if (u.slots != null && status) status.slots = u.slots;
    });
    return () => void unlisten.then((f) => f());
  });

  async function act(f: () => Promise<unknown>) {
    busy = true;
    error = null;
    try {
      await f();
    } catch (e) {
      error = errorMessage(e);
    }
    busy = false;
  }

  const startDaemon = () => act(async () => {
    await api.tourneyStartDaemon(startSlots);
    await refresh();
  });

  function setSlots(n: number) {
    if (n < 0) return;
    void act(() => api.tourneySetSlots(n));
  }

  const shutdown = () => act(async () => {
    await api.tourneyShutdown(false);
  });

  const running = $derived(tests.reduce((n, t) => n + t.running.length, 0));
</script>

<section aria-labelledby="tests-title">
  <h2 id="tests-title">Engine tests</h2>
  {#if status == null}
    <p class="hint">Looking for the test daemon…</p>
  {:else if !status.connected}
    <p class="hint">
      Engine tests play two engines against each other until it's clear which is stronger. They run in a test
      daemon, which goes on with Howdah closed. No daemon is running.
    </p>
    {#if status.canStart}
      <div class="row">
        <label for="tests-slots">Games at once</label>
        <input id="tests-slots" type="number" min="1" bind:value={startSlots} />
        <button onclick={startDaemon} disabled={busy}>Start the test daemon</button>
      </div>
    {:else}
      <p class="hint">Start one with <code>howdah-tourney daemon</code>.</p>
    {/if}
  {:else}
    <div class="row">
      <span class="meta">
        {status.slots ?? '?'} games at once{running > 0 ? `, ${running} playing` : ''}
      </span>
      <button class="subtle" onclick={() => setSlots((status?.slots ?? 1) - 1)} disabled={busy || !status.slots}
        title="One game fewer at once" aria-label="Fewer games at once">−</button>
      <button class="subtle" onclick={() => setSlots((status?.slots ?? 0) + 1)} disabled={busy}
        title="One more game at once" aria-label="More games at once">+</button>
      <span class="spacer"></span>
      <button class="subtle" onclick={shutdown} disabled={busy}
        title="Stop the daemon once its running games end; tests carry on when it starts again">Shut down</button>
      <button class="primary" onclick={() => (showNew = true)}>New test</button>
    </div>
    {#if tests.length === 0}
      <p class="hint">No tests yet.</p>
    {:else}
      <ul class="tests" aria-label="Engine tests">
        {#each [...tests].reverse() as t (t.id)}
          {@const pos = llrPosition(t)}
          <li>
            <button class="test" onclick={() => (shown = t.id)} aria-label="Show test {t.name}">
              <span class="title">
                <span class="name">{t.name}</span>
                <span class="state {testState(t)}">{testState(t)}</span>
              </span>
              <span class="meta">{t.engines[0]} vs {t.engines[1]}</span>
              <span class="meta">
                {scoreText(t)}{t.elo ? `, Elo ${t.elo}` : ''}{t.running.length > 0
                  ? `, ${t.running.length} playing`
                  : ''}
              </span>
              {#if pos != null}
                <span class="llr" title="The SPRT's LLR between its bounds">
                  <span class="mark" style="left: {pos * 100}%"></span>
                </span>
              {/if}
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  {/if}
  {#if status?.dataDir}<p class="hint small">Tests are kept in {status.dataDir}</p>{/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
</section>

{#if showNew}
  <NewTestDialog
    {engines}
    onCreated={(t) => {
      showNew = false;
      shown = t.id;
    }}
    onClose={() => (showNew = false)}
  />
{/if}
{#if shown}
  <TestDialog id={shown} {onOpenGame} onClose={() => (shown = null)} />
{/if}

<style>
  h2 {
    margin: 0 0 10px;
    font-size: 16px;
  }
  .row {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
    margin-bottom: 8px;
    font-size: 13px;
  }
  .row input[type='number'] {
    width: 4em;
  }
  .tests {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .test {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: 2px;
    width: 100%;
    padding: 6px 8px;
    text-align: left;
  }
  .title {
    display: flex;
    align-items: center;
    gap: 6px;
    font-weight: 600;
  }
  .name {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .state {
    margin-left: auto;
    flex: none;
    font-size: 11px;
    font-weight: 400;
    padding: 0 6px;
    border-radius: 8px;
    border: 1px solid var(--border);
    color: var(--muted);
  }
  .state.active {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-text);
  }
  .state.passed {
    color: var(--clock-ok);
    border-color: var(--clock-ok);
  }
  .state.failed,
  .state.halted {
    color: var(--warn);
    border-color: var(--warn);
  }
  .meta {
    color: var(--muted);
    font-size: 12px;
  }
  .llr {
    position: relative;
    height: 4px;
    margin: 4px 0 2px;
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
  .hint {
    margin: 0 0 8px;
    font-size: 12px;
    color: var(--muted);
  }
  .small {
    margin: 8px 0 0;
    overflow-wrap: anywhere;
  }
  .error {
    color: var(--warn);
    font-size: 13px;
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
