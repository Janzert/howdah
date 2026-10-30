<script lang="ts">
  import PieceGlyph from './board/PieceGlyph.svelte';
  import { formatClock, formatFractional, sideTimes } from './clock';
  import { SQ } from './geometry';
  import type { Theme } from './theme';
  import type { ClockView, Color, PieceKind } from './types';

  interface Props {
    side: Color;
    name: string;
    rating?: string | null;
    /** The player is an engine. */
    engine?: boolean;
    /** The clocks shown (live, or worked out for a past move); null for none. */
    clock: ClockView | null;
    /** This side is to move in the shown position. */
    toMove: boolean;
    thinking?: boolean;
    /** The opponent's pieces off the board, strongest first. */
    captured: PieceKind[];
    /** `performance.now()` when `clock` arrived, for local clock ticking. */
    receivedAt: number;
    /** Current `performance.now()`, updated by the parent's ticker. */
    now: number;
    theme: Theme;
    /** The player has left the table (a server's game). */
    away?: boolean;
  }
  let {
    side,
    name,
    rating = null,
    engine = false,
    clock,
    toMove,
    thinking = false,
    captured: lost,
    receivedAt,
    now,
    theme,
    away = false,
  }: Props = $props();

  // The opponent's pieces this side has taken off the board (captures
  // include a side's own pieces lost on traps, so this is material, not who
  // made the capture). Rabbits are grouped with a count, since there can be
  // up to eight; other kinds (at most two) are repeated.
  const opponent = $derived<Color>(side === 'gold' ? 'silver' : 'gold');
  const captured = $derived.by(() => {
    const groups: { kind: PieceKind; n: number }[] = [];
    for (const kind of lost) {
      const last = groups[groups.length - 1];
      if (kind === 'rabbit' && last?.kind === kind) last.n++;
      else groups.push({ kind, n: 1 });
    }
    return groups;
  });
  const capturedLabel = $derived(
    captured.map((g) => (g.n > 1 ? `${g.n} ${opponent} ${g.kind}s` : `${opponent} ${g.kind}`)).join(', '),
  );

  // Colored by the time left for the turn, reserve included: counting
  // down while running, otherwise what the side's next turn will have.
  const times = $derived(clock ? sideTimes(clock, side, now - receivedAt) : null);
  const fractional = $derived(clock?.[side]?.fractional ?? false);
</script>

{#snippet time(ms: number, running: boolean)}
  {#if fractional}
    {@const shown = formatFractional(ms, running)}
    {shown.text}<span class="spare" aria-hidden="true">{shown.spare}</span>
  {:else}
    {formatClock(ms)}
  {/if}
{/snippet}

<div class="bar" class:active={toMove}>
  <span class="dot {side}"></span>
  <span class="name" title={name}>
    {name}
    {#if rating}<span class="rating" title="Rating">{rating}</span>{/if}
    {#if engine}<span class="tag">engine</span>{/if}
    {#if away}<span class="tag away" title="Not at the table; the clock keeps running">away</span>{/if}
  </span>
  {#if thinking}<span class="thinking">thinking…</span>{/if}
  {#if captured.length > 0}
    <span class="captured" role="img" aria-label="captured: {capturedLabel}" title="Captured: {capturedLabel}">
      {#each captured as g, i (i)}
        <span class="cap">
          <svg viewBox="0 0 {SQ} {SQ}" aria-hidden="true">
            <PieceGlyph piece={{ color: opponent, kind: g.kind }} {theme} />
          </svg>
          {#if g.n > 1}<span class="count">×{g.n}</span>{/if}
        </span>
      {/each}
    </span>
  {/if}
  <span class="spacer"></span>
  {#if times}
    <span
      class="clock {times.level}"
      class:running={times.running}
      title={times.used
        ? 'time this move took · reserve after it'
        : clock?.past
          ? 'move time · reserve at this move'
          : 'move time · reserve'}
    >
      <span class="move" class:used={times.used}>{@render time(times.move, times.running)}</span>
      <span class="sep">·</span>
      <span class="reserve">{@render time(times.reserve, times.running)}</span>
    </span>
  {/if}
</div>

<style>
  .bar {
    display: flex;
    flex-wrap: wrap;
    row-gap: 2px;
    align-items: center;
    gap: 8px;
    padding: 4px 8px;
    border-radius: 6px;
    min-height: 28px;
    font-size: 14px;
  }
  .bar.active {
    background: var(--hover);
  }
  .dot {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    border: 1px solid rgba(0, 0, 0, 0.4);
    flex: none;
  }
  .dot.gold {
    background: #e3b23c;
  }
  .dot.silver {
    background: #c9ced6;
  }
  /* At narrow widths the name shortens (it grows from 4em up to its full
     width, ahead of the spacer); only if that's not enough do the captures
     and the clock wrap below, and the captures wrap among themselves only
     when they alone are wider than the bar. */
  .name {
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    flex: 1000 1 4em;
    max-width: max-content;
  }
  .tag.away {
    color: var(--warn);
  }
  .rating {
    font-size: 12px;
    font-weight: 400;
    color: var(--muted);
    margin-left: 4px;
  }
  .tag {
    font-size: 10px;
    font-weight: 400;
    color: var(--muted);
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: 0 3px;
    margin-left: 4px;
  }
  .thinking {
    font-size: 12px;
    color: var(--muted);
    animation: pulse 1.2s ease-in-out infinite;
  }
  @keyframes pulse {
    50% {
      opacity: 0.4;
    }
  }
  .captured {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0 6px;
    min-width: 0;
  }
  .cap {
    display: flex;
    align-items: center;
  }
  .cap svg {
    width: 24px;
    height: 24px;
  }
  .count {
    font-size: 12px;
    font-weight: 600;
    color: var(--muted);
    margin-left: 1px;
  }
  .spacer {
    flex: 1;
  }
  .thinking,
  .clock {
    white-space: nowrap;
    flex: none;
  }
  .clock {
    font-family: ui-monospace, 'DejaVu Sans Mono', monospace;
    font-size: 15px;
    color: var(--muted);
    padding: 1px 6px;
    border-radius: 4px;
  }
  .clock.running {
    color: var(--text);
    background: var(--panel);
    border: 1px solid var(--border);
  }
  /* An idle clock stays muted until it's short of time. */
  .clock.running.ok {
    color: var(--clock-ok);
  }
  .clock.warn {
    color: var(--clock-warn);
  }
  .clock.low {
    color: var(--warn);
  }
  .sep {
    opacity: 0.5;
  }
  .spare {
    visibility: hidden;
  }
  /* The time a past move took stands out from the clocks' other times. */
  .move.used {
    color: var(--text);
  }
</style>
