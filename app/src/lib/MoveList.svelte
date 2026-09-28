<script lang="ts">
  import { tick } from 'svelte';
  import type { SessionView } from './bindings/SessionView';

  let { view, onGoto }: { view: SessionView; onGoto: (ply: number) => void } = $props();

  let list: HTMLOListElement;
  // Stay scrolled to the bottom while following the latest move, unless the
  // user scrolls up; scrolling back to the bottom (or pressing End) resumes it.
  let stick = true;
  let lastPly = -1;

  function onscroll() {
    stick = list.scrollTop + list.clientHeight >= list.scrollHeight - 4;
  }

  $effect(() => {
    // Re-run when moves arrive, the in-progress turn changes, or the ply moves.
    const ply = view.ply;
    const atEnd = ply === view.moves.length;
    void view.moves.length;
    void view.turn?.steps.length;
    tick().then(() => {
      if (!list) return;
      const navigated = ply !== lastPly;
      lastPly = ply;
      if (atEnd && (stick || navigated)) {
        list.scrollTop = list.scrollHeight;
        stick = true;
      } else if (navigated) {
        list.querySelector('.current')?.scrollIntoView({ block: 'nearest' });
      }
    });
  });
</script>

<ol class="moves" bind:this={list} {onscroll}>
  <li>
    <button class="move" class:current={view.ply === 0} onclick={() => onGoto(0)}>
      <span class="label">start</span>
    </button>
  </li>
  {#each view.moves as m (m.ply)}
    <li>
      <button class="move" class:current={view.ply === m.ply} onclick={() => onGoto(m.ply)}>
        <span class="label">{m.label}</span>
        <span class="notation">{m.notation}</span>
      </button>
    </li>
  {/each}
  {#if view.turn && view.turn.steps.length > 0}
    <li class="pending">
      <span class="label">…</span>
      <span class="notation">{view.turn.steps.map((s) => s.notation).join(' ')}</span>
    </li>
  {/if}
  {#if view.endMarker}
    <li class="marker">{view.endMarker}</li>
  {/if}
</ol>

<style>
  .moves {
    list-style: none;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    overflow-x: hidden;
    font-family: ui-monospace, 'DejaVu Sans Mono', monospace;
    font-size: 13px;
  }
  .move,
  .pending {
    display: flex;
    gap: 8px;
    width: 100%;
    padding: 3px 8px;
    text-align: left;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    cursor: pointer;
    border-radius: 4px;
  }
  .move:hover {
    background: var(--hover);
  }
  .move.current {
    background: var(--accent);
    color: var(--accent-text);
  }
  .label {
    flex: 0 0 3.2em;
    color: var(--muted);
  }
  .current .label {
    color: inherit;
  }
  .notation {
    white-space: normal;
    word-break: break-word;
  }
  .pending {
    font-style: italic;
    color: var(--muted);
    cursor: default;
  }
  .marker {
    padding: 3px 8px;
    color: var(--muted);
  }
</style>
