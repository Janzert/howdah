<script lang="ts">
  import PieceGlyph from './board/PieceGlyph.svelte';
  import type { Color } from './bindings/Color';
  import type { PieceKind } from './bindings/PieceKind';
  import type { SessionView } from './bindings/SessionView';
  import { formatClock } from './clock';
  import { SQ } from './geometry';
  import type { Theme } from './theme';

  interface Props {
    view: SessionView;
    side: Color;
    /** `performance.now()` when `view` arrived, for local clock ticking. */
    receivedAt: number;
    /** Current `performance.now()`, updated by the parent's ticker. */
    now: number;
    theme: Theme;
    /** The player has left the table (an arimaa.com game). */
    away?: boolean;
  }
  let { view, side, receivedAt, now, theme, away = false }: Props = $props();

  const player = $derived(view.players?.[side]);
  const clock = $derived(view.clock);
  const toMove = $derived(view.result == null && view.position.sideToMove === side);
  const thinking = $derived(view.thinking === side);
  const rating = $derived(view.tagRatings[side === 'gold' ? 0 : 1]);

  // The opponent's pieces this side has taken off the board (captures
  // include a side's own pieces lost on traps, so this is material, not who
  // made the capture). Rabbits are grouped with a count, since there can be
  // up to eight; other kinds (at most two) are repeated.
  const opponent = $derived<Color>(side === 'gold' ? 'silver' : 'gold');
  const captured = $derived.by(() => {
    const groups: { kind: PieceKind; n: number }[] = [];
    for (const kind of view.captured[opponent]) {
      const last = groups[groups.length - 1];
      if (kind === 'rabbit' && last?.kind === kind) last.n++;
      else groups.push({ kind, n: 1 });
    }
    return groups;
  });
  const capturedLabel = $derived(
    captured.map((g) => (g.n > 1 ? `${g.n} ${opponent} ${g.kind}s` : `${opponent} ${g.kind}`)).join(', '),
  );

  const fmt = formatClock;

  const times = $derived.by(() => {
    const mine = clock?.[side];
    if (!clock || !mine) return null;
    const { moveTimeMs, reserveMs } = mine;
    if (clock.running !== side) return { move: moveTimeMs, reserve: reserveMs, running: false, low: false };
    const elapsed = clock.turnElapsedMs + (now - receivedAt);
    const left = Math.max(0, clock.turnAllowanceMs - elapsed);
    const move = Math.min(left, Math.max(0, moveTimeMs - elapsed));
    const reserveLeft = Math.min(left, Math.max(0, reserveMs - Math.max(0, elapsed - moveTimeMs)));
    return { move, reserve: reserveLeft, running: true, low: left < 10_000 };
  });
</script>

<div class="bar" class:active={toMove}>
  <span class="dot {side}"></span>
  <span class="name">
    {player?.name ?? view.tagNames[side === 'gold' ? 0 : 1] ?? (side === 'gold' ? 'Gold' : 'Silver')}
    {#if rating}<span class="rating" title="Rating">{rating}</span>{/if}
    {#if player?.kind === 'engine'}<span class="tag">engine</span>{/if}
    {#if away}<span class="tag away" title="Not at the table on arimaa.com; the clock keeps running">away</span>{/if}
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
    <span class="clock" class:running={times.running} class:low={times.low} title="move time · reserve">
      <span class="move">{fmt(times.move)}</span>
      <span class="sep">·</span>
      <span class="reserve">{fmt(times.reserve)}</span>
    </span>
  {/if}
</div>

<style>
  .bar {
    display: flex;
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
  .name {
    font-weight: 600;
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
    align-items: center;
    gap: 6px;
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
  .clock.low {
    color: var(--warn);
  }
  .sep {
    opacity: 0.5;
  }
</style>
