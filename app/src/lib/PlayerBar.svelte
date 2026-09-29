<script lang="ts">
  import type { Color } from './bindings/Color';
  import type { SessionView } from './bindings/SessionView';

  interface Props {
    view: SessionView;
    side: Color;
    /** `performance.now()` when `view` arrived, for local clock ticking. */
    receivedAt: number;
    /** Current `performance.now()`, updated by the parent's ticker. */
    now: number;
  }
  let { view, side, receivedAt, now }: Props = $props();

  const player = $derived(view.players?.[side]);
  const clock = $derived(view.clock);
  const toMove = $derived(view.result == null && view.position.sideToMove === side);
  const thinking = $derived(view.thinking === side);

  function fmt(ms: number): string {
    const total = Math.max(0, Math.ceil(ms / 1000));
    const h = Math.floor(total / 3600);
    const m = Math.floor((total % 3600) / 60);
    const s = total % 60;
    const mm = h > 0 ? String(m).padStart(2, '0') : String(m);
    return (h > 0 ? `${h}:` : '') + `${mm}:${String(s).padStart(2, '0')}`;
  }

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
    {player?.name ?? (side === 'gold' ? 'Gold' : 'Silver')}
    {#if player?.kind === 'engine'}<span class="tag">engine</span>{/if}
  </span>
  {#if thinking}<span class="thinking">thinking…</span>{/if}
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
