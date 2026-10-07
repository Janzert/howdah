<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { formatRabbits } from './analysis';
  import { api, errorMessage } from './api';
  import type { Color } from './bindings/Color';
  import type { EngineOption } from './bindings/EngineOption';
  import type { EngineOutput } from './bindings/EngineOutput';
  import type { EngineSpec } from './bindings/EngineSpec';
  import type { PlayersView } from './bindings/PlayersView';
  import { on } from './events';
  import GameOptionsDialog from './GameOptionsDialog.svelte';

  interface Props {
    players: PlayersView;
    /** Changes whenever a new match starts, clearing the output. */
    resetKey: number;
    engines: EngineSpec[];
  }
  let { players, resetKey, engines }: Props = $props();

  /** The side whose engine options are being changed. */
  let optionsFor = $state<Color | null>(null);
  const engineOf = (side: Color) => engines.find((e) => e.id === players[side].engineId);

  async function setOptions(side: Color, options: EngineOption[]): Promise<string | null> {
    try {
      await api.setEngineOptions(side, options);
      return null;
    } catch (e) {
      return errorMessage(e);
    }
  }

  interface SideState {
    depth: string | null;
    score: number | null;
    pv: string[];
    lines: { kind: string; text: string }[];
  }
  const empty = (): SideState => ({ depth: null, score: null, pv: [], lines: [] });
  let output = $state<Record<Color, SideState>>({ gold: empty(), silver: empty() });
  const MAX_LINES = 150;

  $effect(() => {
    resetKey;
    untrack(() => (output = { gold: empty(), silver: empty() }));
  });

  onMount(() => {
    const unlisten = on('engine://output', (o: EngineOutput) => {
      const s = output[o.side];
      if (o.depth != null) s.depth = o.depth;
      if (o.score != null) s.score = o.score;
      if (o.pv != null) s.pv = o.pv;
      s.lines.push({ kind: o.kind, text: o.text });
      if (s.lines.length > MAX_LINES) s.lines.splice(0, s.lines.length - MAX_LINES);
    });
    return () => {
      unlisten.then((f) => f());
    };
  });

  async function press(side: Color, name: string): Promise<string | null> {
    try {
      await api.pressEngineButton(side, name);
      return null;
    } catch (e) {
      return errorMessage(e);
    }
  }

  const sides = $derived((['gold', 'silver'] as Color[]).filter((c) => players[c].kind === 'engine'));

  function scrollToEnd(node: HTMLElement, _lines: unknown) {
    const go = () => (node.scrollTop = node.scrollHeight);
    go();
    return { update: go };
  }
</script>

<div class="panel">
  {#each sides as side (side)}
    {@const s = output[side]}
    <section>
      <header>
        <span class="dot {side}"></span>
        <strong>{players[side].name}</strong>
        {#if s.depth}<span class="stat">depth {s.depth}</span>{/if}
        {#if s.score != null}<span class="stat" title="in rabbits, from this engine's side">eval {formatRabbits(s.score)}</span>{/if}
        <span class="spacer"></span>
        {#if engineOf(side)}
          {@const n = players[side].options.length}
          <button
            class="options"
            title="Change this engine's options for this game"
            onclick={() => (optionsFor = side)}>Options{n ? ` (${n})` : ''}</button
          >
        {/if}
      </header>
      {#if s.pv.length}
        <div class="pv" title="principal variation">{s.pv.join('  |  ')}</div>
      {/if}
      <div class="log" use:scrollToEnd={s.lines.length}>
        {#each s.lines as line, i (i)}
          <div class="line {line.kind}">{line.text}</div>
        {/each}
      </div>
    </section>
  {/each}
</div>
{#if optionsFor}
  {@const side = optionsFor}
  {@const engine = engineOf(side)}
  {#if engine}
    <GameOptionsDialog
      {engine}
      current={players[side].options}
      running
      onApply={(options) => setOptions(side, options)}
      onPress={(name) => press(side, name)}
      onClose={() => (optionsFor = null)}
    />
  {/if}
{/if}

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-height: 0;
  }
  section {
    display: flex;
    flex-direction: column;
    gap: 3px;
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 6px;
    min-height: 0;
  }
  header {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
  }
  .dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    border: 1px solid rgba(0, 0, 0, 0.4);
  }
  .dot.gold {
    background: #e3b23c;
  }
  .dot.silver {
    background: #c9ced6;
  }
  .spacer {
    flex: 1;
  }
  .options {
    font-size: 12px;
    padding: 1px 8px;
  }
  .stat {
    font-family: ui-monospace, 'DejaVu Sans Mono', monospace;
    font-size: 12px;
    color: var(--muted);
  }
  .pv {
    font-family: ui-monospace, 'DejaVu Sans Mono', monospace;
    font-size: 11px;
    word-break: break-word;
  }
  .log {
    font-family: ui-monospace, 'DejaVu Sans Mono', monospace;
    font-size: 11px;
    max-height: 110px;
    overflow-y: auto;
    color: var(--muted);
  }
  .line.status {
    color: var(--accent);
  }
  .line.unexpected {
    color: var(--warn);
  }
</style>
