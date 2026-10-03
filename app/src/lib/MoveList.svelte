<script lang="ts">
  import { tick } from 'svelte';
  import { api } from './api';
  import type { MoveNodeView } from './bindings/MoveNodeView';
  import type { NodeId } from './bindings/NodeId';
  import type { SessionView } from './bindings/SessionView';

  interface Props {
    view: SessionView;
    onGoto: (ply: number) => void;
    onGotoNode: (id: NodeId) => void;
    /** Runs a command, showing its error if it fails. */
    run: (p: Promise<unknown>) => Promise<boolean>;
  }
  let { view, onGoto, onGotoNode, run }: Props = $props();

  let list: HTMLOListElement;
  let lastCursor = -1;

  // Keep the shown move (or the move being entered) in view as it changes.
  $effect(() => {
    const cursor = view.cursor;
    void view.tree.length;
    const steps = view.turn?.steps.length ?? 0;
    tick().then(() => {
      if (!list) return;
      const moved = cursor !== lastCursor;
      lastCursor = cursor;
      const target = list.querySelector(steps > 0 ? '.pending' : '.current');
      if (moved || steps > 0) target?.scrollIntoView({ block: 'nearest' });
    });
  });

  const byId = $derived(new Map(view.tree.map((m) => [m.id, m])));

  /** The live node and its ancestors: a running match's line. */
  const liveLine = $derived.by(() => {
    const ids = new Set<NodeId>();
    const live = view.live == null ? undefined : byId.get(view.live);
    if (!live || live.result != null) return ids;
    for (let m: MoveNodeView | undefined = live; m; m = byId.get(m.parent)) ids.add(m.id);
    return ids;
  });

  /** Alternatives for the same ply, in order. */
  function siblings(m: MoveNodeView): MoveNodeView[] {
    return view.tree.filter((o) => o.parent === m.parent);
  }

  /** The moves from the start to `m`, as record lines. */
  function lineTo(m: MoveNodeView): string {
    const lines: string[] = [];
    for (let n: MoveNodeView | undefined = m; n; n = byId.get(n.parent)) lines.push(`${n.label} ${n.notation}`);
    return lines.reverse().join('\n') + '\n';
  }

  let menu = $state<{ m: MoveNodeView; x: number; y: number } | null>(null);

  function openMenu(e: MouseEvent, m: MoveNodeView) {
    e.preventDefault();
    menu = { m, x: e.clientX, y: e.clientY };
  }

  function act(f: () => Promise<unknown>) {
    // Start the action before closing the menu: the items read the move
    // from it.
    const done = f();
    menu = null;
    run(done);
  }

  function onWindowKey(e: KeyboardEvent) {
    if (menu && e.key === 'Escape') {
      e.stopPropagation();
      menu = null;
    }
  }
</script>

<svelte:window onkeydowncapture={onWindowKey} onclick={() => (menu = null)} />

<ol class="moves" bind:this={list}>
  <li>
    <button class="move" class:current={view.ply === 0} onclick={() => onGoto(0)}>
      <span class="label">start</span>
    </button>
    {#if view.gameComment}<div class="comment game">{view.gameComment}</div>{/if}
  </li>
  {#each view.tree as m (m.id)}
    <li
      class="row"
      class:variation={m.depth > 0}
      class:off-line={!m.onLine}
      class:starts={m.startsVariation}
      style="--depth: {m.depth}"
    >
      {#if m.intro}<div class="comment intro">{m.intro}</div>{/if}
      {#if m.collapsible}
        <button
          class="fold"
          aria-label={m.folded ? `Unfold the variation from ${m.label}` : `Fold the variation from ${m.label}`}
          aria-expanded={m.folded === 0}
          onclick={() => run(api.toggleCollapsed(m.id))}>{m.folded ? '▸' : '▾'}</button
        >
      {/if}
      <button
        class="move"
        class:current={m.id === view.cursor}
        class:live={m.id === view.live}
        onclick={() => onGotoNode(m.id)}
        oncontextmenu={(e) => openMenu(e, m)}
      >
        <span class="label">{m.label}</span>
        <span class="notation">{m.notation}{#if m.glyphs.length}<span class="glyphs">{m.glyphs.join(' ')}</span>{/if}{#if m.folded}<span class="folded">+{m.folded}</span>{/if}</span>
      </button>
      {#if m.comment}<div class="comment">{m.comment}</div>{/if}
      {#if m.id === view.cursor && view.turn && view.turn.steps.length > 0}
        <div class="pending">
          <span class="label">…</span>
          <span class="notation">{view.turn.steps.map((s) => s.notation).join(' ')}</span>
        </div>
      {/if}
    </li>
  {/each}
  {#if view.ply === 0 && view.turn && view.turn.steps.length > 0}
    <li class="pending">
      <span class="label">…</span>
      <span class="notation">{view.turn.steps.map((s) => s.notation).join(' ')}</span>
    </li>
  {/if}
  {#if view.endMarker}
    <li class="marker">{view.endMarker}</li>
  {/if}
</ol>

{#if menu}
  {@const m = menu.m}
  {@const sibs = siblings(m)}
  {@const index = sibs.findIndex((s) => s.id === m.id)}
  {@const protectedLine = liveLine.has(m.id)}
  <div class="menu" role="menu" style="left: {menu.x}px; top: {menu.y}px">
    <button role="menuitem" onclick={() => act(() => api.makeMainLine(m.id))}>Make main line</button>
    {#if m.collapsible}
      <button role="menuitem" onclick={() => act(() => api.toggleCollapsed(m.id))}>
        {m.folded ? 'Unfold variation' : 'Fold variation'}
      </button>
    {/if}
    <button role="menuitem" disabled={index <= 0} onclick={() => act(() => api.promote(m.id))}>Move up</button>
    <button role="menuitem" disabled={index < 0 || index === sibs.length - 1} onclick={() => act(() => api.demote(m.id))}>
      Move down
    </button>
    <button role="menuitem" disabled={protectedLine} onclick={() => act(() => api.deleteFrom(m.id))}>
      Delete from here
    </button>
    <hr />
    <button role="menuitem" onclick={() => act(() => navigator.clipboard.writeText(m.notation))}>Copy move</button>
    <button role="menuitem" onclick={() => act(() => navigator.clipboard.writeText(lineTo(m)))}>
      Copy moves to here
    </button>
  </div>
{/if}

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
  .row {
    /* Variations are indented one step per level, with a guide line. */
    position: relative;
    margin-left: calc(var(--depth) * 14px);
  }
  .fold {
    position: absolute;
    left: -14px;
    top: 2px;
    width: 14px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--muted);
    font-size: 10px;
    line-height: 18px;
    cursor: pointer;
  }
  .fold:hover {
    color: inherit;
  }
  .folded {
    color: var(--muted);
  }
  .row.variation {
    border-left: 2px solid var(--border);
    font-size: 12px;
  }
  .row.starts {
    margin-top: 2px;
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
  .off-line > .move {
    color: var(--muted);
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
  .glyphs,
  .folded {
    margin-left: 0.4em;
  }
  .glyphs {
    font-weight: bold;
  }
  .comment {
    padding: 0 8px 3px calc(8px + 3.2em + 8px);
    font-family: system-ui, sans-serif;
    font-size: 12px;
    color: var(--muted);
    white-space: pre-wrap;
  }
  .comment.game {
    padding-left: 8px;
  }
  .comment.intro {
    padding-top: 2px;
    padding-left: 8px;
    font-style: italic;
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
  .menu {
    position: fixed;
    z-index: 50;
    display: flex;
    flex-direction: column;
    min-width: 180px;
    padding: 4px;
    background: var(--panel);
    color: var(--text);
    border: 1px solid var(--border);
    border-radius: 6px;
    box-shadow: 0 4px 16px rgb(0 0 0 / 0.3);
    font-size: 13px;
  }
  .menu button {
    padding: 5px 10px;
    text-align: left;
    border: 0;
    border-radius: 4px;
    background: none;
    color: inherit;
    font: inherit;
    cursor: pointer;
  }
  .menu button:hover:not(:disabled) {
    background: var(--hover);
  }
  .menu button:disabled {
    color: var(--muted);
    cursor: default;
  }
  .menu hr {
    width: 100%;
    margin: 4px 0;
    border: 0;
    border-top: 1px solid var(--border);
  }
</style>
