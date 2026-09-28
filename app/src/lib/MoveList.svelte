<script lang="ts">
  import type { SessionView } from './bindings/SessionView';

  let { view, onGoto }: { view: SessionView; onGoto: (ply: number) => void } = $props();

  let list: HTMLOListElement;

  // Keep the current move in view while stepping through a game.
  $effect(() => {
    const el = list?.querySelector('.current');
    el?.scrollIntoView({ block: 'nearest' });
  });
</script>

<ol class="moves" bind:this={list}>
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
