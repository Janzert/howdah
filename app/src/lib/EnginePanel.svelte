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
    /** Changes whenever a new match starts, reloading the output. */
    resetKey: number;
    engines: EngineSpec[];
    /** An engine test's game: both sides are engines the daemon runs. */
    daemon?: boolean;
  }
  let { players, resetKey, engines, daemon = false }: Props = $props();

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
  /** Whether each side's log is shown. */
  let logOpen = $state<Record<Color, boolean>>({ gold: false, silver: false });
  const MAX_LINES = 150;

  /** The newest output shown, by its number. */
  let shownSeq = 0;
  /** Output arriving while the kept output loads, to add after it. */
  let pending: EngineOutput[] | null = null;
  let loads = 0;

  function take(o: EngineOutput) {
    if (o.seq <= shownSeq) return;
    shownSeq = o.seq;
    const s = output[o.side];
    if (o.depth != null) s.depth = o.depth;
    if (o.score != null) s.score = o.score;
    if (o.pv != null) s.pv = o.pv;
    s.lines.push({ kind: o.kind, text: o.text });
    if (s.lines.length > MAX_LINES) s.lines.splice(0, s.lines.length - MAX_LINES);
  }

  /** Starts over with the output the session kept for its game, which
   * came before the panel opened or the game changed. */
  async function load() {
    const mine = ++loads;
    output = { gold: empty(), silver: empty() };
    shownSeq = 0;
    pending = [];
    let kept: EngineOutput[] = [];
    try {
      kept = await api.engineOutput();
    } catch {
      /* the live output still comes */
    }
    if (mine !== loads) return;
    const arrived = pending;
    pending = null;
    for (const o of [...kept, ...arrived]) take(o);
  }

  $effect(() => {
    resetKey;
    untrack(() => void load());
  });

  onMount(() => {
    const unlisten = on('engine://output', (o: EngineOutput) => {
      if (pending) pending.push(o);
      else take(o);
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

  const sides = $derived((['gold', 'silver'] as Color[]).filter((c) => daemon || players[c].kind === 'engine'));

  function scrollToEnd(node: HTMLElement, _lines: unknown) {
    const go = () => (node.scrollTop = node.scrollHeight);
    go();
    // The log starts folded away, with nothing to scroll until it opens.
    const details = node.closest('details');
    details?.addEventListener('toggle', go);
    return { update: go, destroy: () => details?.removeEventListener('toggle', go) };
  }
</script>

<div class="panel">
  {#each sides as side (side)}
    {@const open = logOpen[side]}
    {@const s = output[side]}
    <section>
      <header>
        <span class="dot {side}"></span>
        <strong class="name" title={players[side].name}>{players[side].name}</strong>
        {#if s.depth}<span class="stat">depth {s.depth}</span>{/if}
        {#if s.score != null}<span class="stat" title="in rabbits, from this engine's side"
            >eval {formatRabbits(s.score)}</span
          >{/if}
        <span class="spacer"></span>
        <button
          class="options"
          aria-expanded={open}
          onclick={() => (logOpen[side] = !open)}
          title="This engine's output"
          aria-label="{players[side].name}'s log">Log</button
        >
        {#if engineOf(side)}
          {@const n = players[side].options.length}
          <button class="options" title="Change this engine's options for this game" onclick={() => (optionsFor = side)}
            >Options{n ? ` (${n})` : ''}</button
          >
        {/if}
      </header>
      {#if s.pv.length}
        <div class="pv" title="principal variation">{s.pv.join('  |  ')}</div>
      {/if}
      {#if open}
        <div class="log" use:scrollToEnd={s.lines.length}>
          {#each s.lines as line, i (i)}
            <div class="line {line.kind}">{line.text}</div>
          {/each}
        </div>
      {/if}
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
    padding: 4px 6px;
    min-height: 0;
  }
  .name {
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  header > :not(.name) {
    flex: none;
  }
  .options[aria-expanded='true'] {
    background: var(--hover);
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
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
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
