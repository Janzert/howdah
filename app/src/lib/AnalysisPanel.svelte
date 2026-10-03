<script lang="ts">
  // The analysis section of the side panel: the engine, depth, eval and
  // speed, the principal variation as one chip per turn (hover previews the
  // position after it, click adds the PV up to it), and the engine's output.
  import { onMount } from 'svelte';
  import { formatEval, nodesPerSecond, pvMoves } from './analysis';
  import type { AnalysisLine } from './bindings/AnalysisLine';
  import type { AnalysisView } from './bindings/AnalysisView';
  import type { EngineSpec } from './bindings/EngineSpec';
  import type { PositionView } from './bindings/PositionView';
  import type { SessionView } from './bindings/SessionView';
  import MiniBoard from './board/MiniBoard.svelte';
  import { on } from './events';
  import type { Theme } from './theme';

  interface Props {
    view: SessionView;
    /** The latest update from the engine. */
    analysis: AnalysisView | null;
    /** The line shown for the board's node, and whether it's from an earlier visit. */
    line: AnalysisLine | null;
    stored: boolean;
    engines: EngineSpec[];
    theme: Theme;
    flipped: boolean;
    /** Match engines are thinking too, on the same CPU. */
    sharesCpu: boolean;
    onEngine: (id: string) => void;
    onClose: () => void;
    /** Adds the PV up to turn `index` as a line. */
    onAdd: (line: AnalysisLine, index: number) => void;
    preview: (line: AnalysisLine, index: number) => Promise<PositionView>;
  }
  let { view, analysis, line, stored, engines, theme, flipped, sharesCpu, onEngine, onClose, onAdd, preview }: Props =
    $props();

  const MAX_LINES = 200;
  let log = $state<string[]>([]);
  onMount(() => {
    const unlisten = on('analysis://update', (u) => {
      if (u.log.length === 0) return;
      log.push(...u.log);
      if (log.length > MAX_LINES) log.splice(0, log.length - MAX_LINES);
    });
    return () => {
      unlisten.then((f) => f());
    };
  });

  const phase = $derived(analysis?.state ?? 'starting');
  const nps = $derived(line ? nodesPerSecond(line) : null);
  const beforeSteps = $derived((view.turn?.steps.length ?? 0) > 0);

  const status = $derived.by(() => {
    switch (phase) {
      case 'starting':
        return `Starting ${analysis?.engine ?? 'the engine'}…`;
      case 'searching':
      case 'finished': {
        const parts = [phase === 'finished' ? `Done: ${analysis?.label ?? ''}` : `Searching ${analysis?.label ?? ''}`];
        if (stored && line?.depth) parts.push(`stored, depth ${line.depth}`);
        if (beforeSteps) parts.push('before your steps');
        return parts.join(' · ');
      }
      case 'idle':
        return 'The game is over here.';
      case 'failed':
        return analysis?.detail ?? 'The engine failed.';
      case 'off':
        return 'Off';
    }
  });

  // The hover preview: the position after a PV turn.
  let hovered = $state<{ index: number; top: number; position: PositionView | null } | null>(null);
  const cache = new Map<string, PositionView>();
  /** The preview's height in pixels: a 220px board, padding and caption. */
  const PREVIEW_HEIGHT = 250;
  let panel: HTMLElement;

  async function hover(index: number, chip: HTMLElement) {
    if (!line) return;
    const l = line;
    // Beside the chip, but kept inside the window.
    const top =
      Math.max(8, Math.min(chip.getBoundingClientRect().top, window.innerHeight - PREVIEW_HEIGHT - 8)) -
      panel.getBoundingClientRect().top;
    const key = `${l.node}|${pvMoves(l, index).join(',')}`;
    hovered = { index, top, position: cache.get(key) ?? null };
    if (hovered.position) return;
    try {
      const position = await preview(l, index);
      cache.set(key, position);
      if (hovered?.index === index && line === l) hovered = { index, top, position };
    } catch {
      /* the line changed under the pointer; no preview */
    }
  }

  function scrollToEnd(node: HTMLElement, _lines: unknown) {
    const go = () => (node.scrollTop = node.scrollHeight);
    go();
    return { update: go };
  }
</script>

<section class="analysis" bind:this={panel} aria-label="Analysis">
  <header>
    <strong>Analysis</strong>
    <select
      aria-label="Analysis engine"
      value={view.analysisEngine ?? ''}
      onchange={(e) => onEngine((e.target as HTMLSelectElement).value)}
    >
      {#if !view.analysisEngine}<option value="" disabled>engine…</option>{/if}
      {#each engines as e (e.id)}
        <option value={e.id}>{e.name}</option>
      {/each}
    </select>
    <span class="spacer"></span>
    <button class="close" onclick={onClose} aria-label="Turn analysis off" title="Turn analysis off (l)">×</button>
  </header>
  {#if line && phase !== 'failed'}
    <div class="stats">
      {#if line.eval}<span class="eval" title="from gold's side, in rabbits">{formatEval(line.eval)}</span>{/if}
      {#if line.depth}<span>depth {line.depth}</span>{/if}
      {#if nps}<span>{nps} n/s</span>{/if}
    </div>
  {/if}
  <p class="status" class:failed={phase === 'failed'} role="status">{status}</p>
  {#if sharesCpu}<p class="note">Shares the CPU with the match engines.</p>{/if}
  {#if line && line.pv.length && phase !== 'failed'}
    <div class="pv" aria-label="Principal variation">
      {#each line.pv as turn, i (i)}
        <button
          class="chip"
          title="Add the line up to here"
          onclick={() => onAdd(line, i)}
          onmouseenter={(e) => hover(i, e.currentTarget)}
          onmouseleave={() => (hovered = null)}
          onfocus={(e) => hover(i, e.currentTarget)}
          onblur={() => (hovered = null)}
        >
          <span class="label">{turn.label}</span>
          {turn.notation}
        </button>
      {/each}
    </div>
  {/if}
  {#if hovered?.position}
    <div class="preview" style:top="{hovered.top}px" aria-hidden="true">
      <MiniBoard position={hovered.position} {theme} {flipped} />
      <div class="caption">after {line?.pv[hovered.index]?.label}</div>
    </div>
  {/if}
  {#if log.length}
    <details>
      <summary>Engine output</summary>
      <div class="log" use:scrollToEnd={log.length}>
        {#each log as text, i (i)}
          <div>{text}</div>
        {/each}
      </div>
    </details>
  {/if}
</section>

<style>
  .analysis {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 4px;
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 6px;
    font-size: 13px;
  }
  header {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  select {
    font: inherit;
    font-size: 12px;
    max-width: 150px;
    padding: 1px 4px;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--bg);
    color: var(--text);
  }
  .spacer {
    flex: 1;
  }
  .close {
    padding: 0 6px;
    line-height: 1.3;
  }
  .stats {
    display: flex;
    gap: 10px;
    align-items: baseline;
    font-family: ui-monospace, 'DejaVu Sans Mono', monospace;
    font-size: 12px;
    color: var(--muted);
  }
  .eval {
    font-size: 15px;
    font-weight: 600;
    color: var(--text);
  }
  .status,
  .note {
    margin: 0;
    font-size: 12px;
    color: var(--muted);
  }
  .status.failed {
    color: var(--warn);
  }
  .pv {
    display: flex;
    flex-wrap: wrap;
    gap: 3px;
    max-height: 120px;
    overflow-y: auto;
  }
  .chip {
    font-family: ui-monospace, 'DejaVu Sans Mono', monospace;
    font-size: 11px;
    padding: 1px 5px;
    border-radius: 4px;
  }
  .chip .label {
    color: var(--muted);
  }
  .preview {
    position: absolute;
    right: calc(100% + 18px);
    width: 220px;
    z-index: 10;
    padding: 4px;
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 6px;
    box-shadow: 0 6px 18px rgba(0, 0, 0, 0.25);
    pointer-events: none;
  }
  .preview :global(svg) {
    aspect-ratio: 1;
  }
  .caption {
    text-align: center;
    font-size: 11px;
    color: var(--muted);
  }
  details summary {
    font-size: 12px;
    color: var(--muted);
    cursor: pointer;
  }
  .log {
    font-family: ui-monospace, 'DejaVu Sans Mono', monospace;
    font-size: 11px;
    max-height: 110px;
    overflow-y: auto;
    color: var(--muted);
    word-break: break-word;
  }
</style>
