<script lang="ts">
  import { untrack } from 'svelte';
  import { api } from './api';
  import type { NodeId } from './bindings/NodeId';
  import type { SessionView } from './bindings/SessionView';

  interface Props {
    view: SessionView;
    /** Runs a command, showing its error if it fails. */
    run: (p: Promise<unknown>) => Promise<boolean>;
  }
  let { view, run }: Props = $props();

  /** The move glyphs, numbered as in PGN. */
  const GLYPHS: { n: number; symbol: string; title: string }[] = [
    { n: 1, symbol: '!', title: 'Good move' },
    { n: 2, symbol: '?', title: 'Mistake' },
    { n: 3, symbol: '!!', title: 'Brilliant move' },
    { n: 4, symbol: '??', title: 'Blunder' },
    { n: 5, symbol: '!?', title: 'Interesting move' },
    { n: 6, symbol: '?!', title: 'Dubious move' },
  ];

  const move = $derived(view.tree.find((m) => m.id === view.cursor) ?? null);

  /** The root, whose comment is the game comment. */
  const ROOT: NodeId = 0;

  function commentOf(id: NodeId): string {
    if (id === ROOT) return view.gameComment ?? '';
    return view.tree.find((m) => m.id === id)?.comment ?? '';
  }

  let draft = $state('');
  let focused = false;
  /** The node the draft belongs to: the board can move on while typing. */
  let editing: NodeId | null = null;
  let box: HTMLTextAreaElement;

  // Show the shown move's comment on every update, unless it's being edited.
  $effect(() => {
    const cursor = view.cursor;
    const comment = commentOf(cursor);
    untrack(() => {
      if (!focused) show(cursor, comment);
    });
  });

  function show(cursor: NodeId, comment: string) {
    draft = comment;
    editing = cursor;
  }

  /** Saves a changed comment; the update that follows shows it. */
  function save() {
    focused = false;
    if (editing != null && draft.trimEnd() !== commentOf(editing)) run(api.setComment(editing, draft));
    else show(view.cursor, commentOf(view.cursor));
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      box.blur();
    } else if (e.key === 'Escape') {
      e.preventDefault();
      if (editing != null) draft = commentOf(editing);
      box.blur();
    }
  }
</script>

<div class="annotate">
  {#if move}
    <div class="glyphs" role="group" aria-label="Glyphs for {move.label}">
      {#each GLYPHS as g (g.n)}
        <button
          class:on={move.glyphs.includes(g.symbol)}
          aria-pressed={move.glyphs.includes(g.symbol)}
          title={g.title}
          aria-label={g.title}
          onclick={() => run(api.toggleGlyph(move.id, g.n))}>{g.symbol}</button
        >
      {/each}
    </div>
  {/if}
  <textarea
    bind:this={box}
    bind:value={draft}
    rows="2"
    placeholder={move ? `Comment on ${move.label}` : 'Game comment'}
    aria-label={move ? `Comment on ${move.label} ${move.notation}` : 'Game comment'}
    title="Saved when you leave the box or press Ctrl+Enter; Esc undoes the change"
    onfocus={() => (focused = true)}
    onblur={save}
    {onkeydown}
  ></textarea>
</div>

<style>
  .annotate {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .glyphs {
    display: flex;
    gap: 2px;
  }
  .glyphs button {
    flex: 1;
    padding: 2px 0;
    font-weight: bold;
    font-size: 12px;
  }
  .glyphs button.on {
    background: var(--accent);
    color: var(--accent-text);
  }
  textarea {
    width: 100%;
    box-sizing: border-box;
    resize: vertical;
    min-height: 2.6em;
    padding: 4px 6px;
    font: 12px system-ui, sans-serif;
    background: var(--bg);
    color: inherit;
    border: 1px solid var(--border);
    border-radius: 4px;
  }
</style>
