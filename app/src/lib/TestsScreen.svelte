<script lang="ts">
  // The lobby window's Engine tests screen: the howdah-tourney daemon's queue of tests
  // (docs/TOURNAMENTS.md). The daemon runs on its own, so tests go on with
  // Howdah closed; this lists them (state in `tourney.svelte.ts`) and
  // starts new ones; each opens in a panel of its own (`TestPanel`). Without
  // a daemon it offers to start one. Finished tests can be archived
  // (zipped into the daemon's archive/), one at a time from a test's panel
  // or all at once here, and restored or deleted from the Archived tests
  // dialog.
  import { api, errorMessage } from './api';
  import type { EngineSpec } from './bindings/EngineSpec';
  import type { TestView } from './bindings/TestView';
  import Icon from './Icon.svelte';
  import ArchiveDialog from './ArchiveDialog.svelte';
  import NewTestDialog from './NewTestDialog.svelte';
  import { endsText, llrPosition, scoreText, testState } from './tests';
  import { tourney } from './tourney.svelte';

  interface Props {
    engines: EngineSpec[];
    /** Opens a test's panel. */
    onOpenTest: (id: string) => void;
  }
  let { engines, onOpenTest }: Props = $props();

  const status = $derived(tourney.status);
  const tests = $derived(tourney.tests);
  let error = $state<string | null>(null);
  let busy = $state(false);
  let showNew = $state(false);
  /** What the last archive, delete or restore did (a test's panel's
   * too). */
  const notice = $derived(tourney.notice);
  /** Archive finished was clicked once; the second click archives. */
  let confirmArchive = $state(false);
  let showArchive = $state(false);
  let startSlots = $state(Math.max(1, Math.floor((navigator.hardwareConcurrency || 2) / 2)));

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

  const startDaemon = () =>
    act(async () => {
      await api.tourneyStartDaemon(startSlots);
      await tourney.refresh();
    });

  function setSlots(n: number) {
    if (n < 0) return;
    void act(() => api.tourneySetSlots(n));
  }

  const shutdown = () =>
    act(async () => {
      await api.tourneyShutdown(false);
    });

  const running = $derived(tourney.playing);
  const finished = $derived(tests.filter((t) => t.status === 'finished' && t.running.length === 0));

  /** The tests, newest first: those still going, then the finished. */
  const groups = $derived.by(() => {
    const newest = [...tests].reverse();
    const done = (t: TestView) => t.status === 'finished' && t.running.length === 0;
    return [
      { id: 'going', title: 'In progress', tests: newest.filter((t) => !done(t)) },
      { id: 'finished', title: 'Finished', tests: newest.filter(done) },
    ];
  });

  function archiveFinished() {
    if (!confirmArchive) {
      confirmArchive = true;
      return;
    }
    confirmArchive = false;
    const ids = finished.map((t) => t.id);
    void act(async () => {
      tourney.notice = null;
      let done = 0;
      try {
        for (const id of ids) {
          await api.tourneyRemove(id, true);
          tourney.forget(id);
          done++;
        }
      } finally {
        if (done > 0) tourney.notice = `Archived ${done === 1 ? '1 test' : `${done} tests`} to ${archiveDir()}`;
      }
    });
  }

  /** The daemon's archive folder, for messages. */
  function archiveDir(): string {
    const dir = status?.dataDir;
    if (!dir) return 'the archive folder';
    return `${dir}${dir.includes('\\') ? '\\' : '/'}archive`;
  }
</script>

<div class="screen">
  <header>
    <h1>Engine tests</h1>
    <span class="spacer"></span>
    {#if status?.connected}
      <button onclick={() => (showArchive = true)} title="The archived tests, to restore or delete"
        >Archived tests</button
      >
      <button class="primary icon" onclick={() => (showNew = true)}><Icon name="plus" size={16} /> New test</button>
    {/if}
  </header>

  <section class="card daemon" aria-label="Test daemon">
    {#if status == null}
      <p class="empty">Looking for the test daemon…</p>
    {:else if !status.connected}
      <p class="about">
        Engine tests play two engines against each other until it's clear which is stronger. They run in a test daemon,
        which goes on with Howdah closed. No daemon is running.
      </p>
      {#if status.canStart}
        <div class="fields">
          <label for="tests-slots">Games at once</label>
          <input id="tests-slots" type="number" min="1" bind:value={startSlots} />
          <button class="primary" onclick={startDaemon} disabled={busy}>Start the test daemon</button>
        </div>
      {:else}
        <p class="hint">Start one with <code>howdah-tourney daemon</code>.</p>
      {/if}
    {:else}
      <div class="fields">
        <span class="state-dot" aria-hidden="true"></span>
        <span>Test daemon running</span>
        <span class="sep">·</span>
        <span class="slots">
          <button
            class="subtle"
            onclick={() => setSlots((status?.slots ?? 1) - 1)}
            disabled={busy || !status.slots}
            title="One game fewer at once"
            aria-label="Fewer games at once">−</button
          >
          <span>{status.slots ?? '?'} {status.slots === 1 ? 'game' : 'games'} at once</span>
          <button
            class="subtle"
            onclick={() => setSlots((status?.slots ?? 0) + 1)}
            disabled={busy}
            title="One more game at once"
            aria-label="More games at once">+</button
          >
        </span>
        {#if running > 0}<span class="sep">·</span><span>{running} playing</span>{/if}
        <span class="spacer"></span>
        <button
          class="subtle"
          onclick={shutdown}
          disabled={busy}
          title="Stop the daemon once its running games end; tests carry on when it starts again">Shut down</button
        >
      </div>
    {/if}
    {#if status?.dataDir}<p class="hint small">Tests are kept in {status.dataDir}</p>{/if}
  </section>

  {#if notice}<p class="hint notice" role="status">{notice}</p>{/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}

  {#if status?.connected}
    {#if tests.length === 0}
      <p class="empty">No tests yet. A new test plays two of your engines against each other.</p>
    {:else}
      {#each groups as g (g.title)}
        {#if g.tests.length > 0}
          <section class="group" aria-labelledby="tests-{g.id}">
            <div class="card-head">
              <h2 id="tests-{g.id}">{g.title} <span class="count">{g.tests.length}</span></h2>
              <span class="spacer"></span>
              {#if g.id === 'finished'}
                <button
                  class="subtle"
                  class:danger={confirmArchive}
                  onclick={archiveFinished}
                  onblur={() => (confirmArchive = false)}
                  disabled={busy || finished.length === 0}
                  title="Zip every finished test into the daemon's archive folder and take them off the list"
                  >{confirmArchive ? `Archive ${finished.length} tests?` : 'Archive finished'}</button
                >
              {/if}
            </div>
            <ul class="tests" aria-label="{g.title} engine tests">
              {#each g.tests as t (t.id)}
                {@const pos = llrPosition(t)}
                <li>
                  <button class="test" onclick={() => onOpenTest(t.id)} aria-label="Show test {t.name}">
                    <span class="title">
                      <span class="name">{t.name}</span>
                      <span class="state {testState(t)}">{testState(t)}</span>
                    </span>
                    <span class="meta">{t.engines[0]} vs {t.engines[1]}</span>
                    <span class="meta">
                      {scoreText(t, false)}{t.elo ? `, Elo ${t.elo}` : ''}{t.running.length > 0
                        ? `, ${t.running.length} playing`
                        : ''}
                    </span>
                    {#if endsText(t)}<span class="meta">Ends {endsText(t)}</span>{/if}
                    {#if pos != null}
                      <span class="llr" title="The SPRT's LLR between its bounds">
                        <span class="mark" style="left: {pos * 100}%"></span>
                      </span>
                    {/if}
                  </button>
                </li>
              {/each}
            </ul>
          </section>
        {/if}
      {/each}
    {/if}
  {/if}
</div>

{#if showArchive}
  <ArchiveDialog
    dataDir={status?.dataDir ?? null}
    onRestored={(t) => {
      tourney.add(t);
      tourney.notice = `Restored ${t.id}`;
    }}
    onClose={() => (showArchive = false)}
  />
{/if}
{#if showNew}
  <NewTestDialog
    {engines}
    onCreated={(t) => {
      showNew = false;
      onOpenTest(t.id);
    }}
    onClose={() => (showNew = false)}
  />
{/if}

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
  }
  .daemon {
    margin-bottom: 18px;
  }
  .daemon .about {
    margin: 0 0 10px;
    font-size: 13px;
  }
  .daemon input[type='number'] {
    flex: none;
    width: 4em;
    min-width: 0;
  }
  .state-dot {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: var(--clock-ok);
  }
  .sep {
    color: var(--muted);
  }
  .slots {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .small {
    margin-top: 8px;
    overflow-wrap: anywhere;
  }
  .notice,
  .error {
    margin: -6px 0 14px;
  }
  .group + .group {
    margin-top: 20px;
  }
  .tests {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: 10px;
  }
  .test {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: 3px;
    width: 100%;
    height: 100%;
    padding: 10px 12px;
    text-align: left;
    border-radius: 10px;
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
  .llr {
    position: relative;
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
</style>
