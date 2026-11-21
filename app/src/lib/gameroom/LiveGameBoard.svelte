<script lang="ts">
  // The live game shown small beside the chat room's log (`livePreview`):
  // the players with their clocks, the position, and how the game stands,
  // with Watch to open it on a board and a button to hide it.
  import type { Color } from '../bindings/Color';
  import MiniBoard from '@howdah/board/board/MiniBoard.svelte';
  import { formatClock, sideTimes } from '@howdah/board/clock';
  import { findTheme } from '@howdah/board/theme';
  import Icon from '../Icon.svelte';
  import { barProps } from '../playerBar';
  import { describeResult } from '../result';
  import { settings } from '../settings.svelte';
  import { livePreview } from './livePreview.svelte';
  import { gameroom } from './store.svelte';

  const theme = $derived(findTheme(settings.theme));
  const view = $derived(livePreview.view);
  const watch = $derived(livePreview.watch);

  // Moves slide at the board's speed setting.
  $effect(() => livePreview.model.setBaseSpeed(settings.stepMs));

  let now = $state(performance.now());
  $effect(() => {
    if (!view?.shownClock?.running) return;
    const timer = setInterval(() => (now = performance.now()), 500);
    return () => clearInterval(timer);
  });

  function player(side: Color) {
    if (!view) return null;
    const bar = barProps(view, side);
    const times = bar.clock && sideTimes(bar.clock, side, now - livePreview.at);
    return {
      ...bar,
      clock: times ? `${formatClock(times.move)} · ${formatClock(times.reserve)}` : null,
      running: times?.running ?? false,
    };
  }

  const status = $derived.by(() => {
    if (livePreview.error) return livePreview.error;
    if (!view) return 'Opening the game…';
    if (view.result) return describeResult(view.result).title;
    if (watch?.state === 'reconnecting') return 'Reconnecting to arimaa.com…';
    if (watch?.state === 'failed') return watch.detail ?? 'Lost the game';
    return null;
  });

  function watchOnBoard() {
    const gid = livePreview.gid;
    if (gid) void gameroom.open(gid);
  }
</script>

{#snippet bar(side: Color)}
  {@const p = player(side)}
  <div class="bar" class:to-move={p?.toMove}>
    <span class="dot {side}"></span>
    <span class="name">{p?.name ?? (side === 'gold' ? 'Gold' : 'Silver')}</span>
    {#if p?.rating}<span class="rating">{p.rating}</span>{/if}
    <span class="spacer"></span>
    {#if p?.clock}<span class="clock" class:running={p.running}>{p.clock}</span>{/if}
  </div>
{/snippet}

{#if livePreview.gid}
  <aside class="live-board" aria-label="Game {livePreview.gid}">
    {@render bar('silver')}
    <div class="board">
      {#if view}<MiniBoard position={view.position} {theme} flipped={false} model={livePreview.model} />{/if}
    </div>
    {@render bar('gold')}
    {#if status}<p class="status" role="status">{status}</p>{/if}
    <div class="actions">
      <button onclick={watchOnBoard} disabled={gameroom.busy || !view} title="Open the game on a board">Watch</button>
      <span class="spacer"></span>
      <button
        class="bare close"
        onclick={() => livePreview.close()}
        title="Hide the board"
        aria-label="Hide game {livePreview.gid}'s board"><Icon name="close" size={14} /></button
      >
    </div>
  </aside>
{/if}

<style>
  .live-board {
    flex: none;
    display: flex;
    flex-direction: column;
    gap: 4px;
    width: 240px;
    font-size: 13px;
  }
  .board {
    aspect-ratio: 1;
    border-radius: 4px;
    overflow: hidden;
    background: var(--panel);
  }
  .bar {
    display: flex;
    align-items: baseline;
    gap: 5px;
    min-width: 0;
    padding: 2px 6px;
    border-radius: 6px;
  }
  .bar.to-move {
    background: var(--hover);
    font-weight: 600;
  }
  .name {
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .rating {
    color: var(--muted);
    font-size: 12px;
    font-weight: 400;
  }
  .spacer {
    flex: 1;
  }
  .clock {
    font-variant-numeric: tabular-nums;
    color: var(--muted);
    font-weight: 400;
    white-space: nowrap;
  }
  .clock.running {
    color: var(--text);
  }
  .status {
    margin: 0;
    color: var(--muted);
  }
  .actions {
    display: flex;
    align-items: center;
  }
  .actions button:not(.bare) {
    padding: 2px 10px;
    font-size: 12px;
  }
  .close {
    display: flex;
    padding: 4px;
    border-radius: 6px;
    color: var(--muted);
  }
</style>
