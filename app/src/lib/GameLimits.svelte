<script lang="ts">
  // The game time limit and turn limit, when the time control has them:
  // the time left before the game is decided by score, counted down while
  // a clock runs, and the turns left.
  import { formatClock, gameTimeLeft } from '@howdah/board/clock';
  import type { ClockView } from './bindings/ClockView';

  interface Props {
    /** The clocks shown (live, or worked out for a past move). */
    clock: ClockView | null;
    /** `performance.now()` when `clock` arrived. */
    receivedAt: number;
    /** Current `performance.now()`, from the parent's ticker. */
    now: number;
  }
  let { clock, receivedAt, now }: Props = $props();

  const gameLeft = $derived(clock ? gameTimeLeft(clock, now - receivedAt) : null);
  const turnsLeft = $derived(clock?.turnsLeft ?? null);
  const when = $derived(clock?.past ? ' at this move' : '');
</script>

{#if gameLeft != null || turnsLeft != null}
  <div class="limits">
    {#if gameLeft != null}
      <span
        class:warn={gameLeft < 5 * 60_000}
        title="Time left before the game time limit{when}; then the game is decided by score"
      >
        Game time <span class="time">{formatClock(gameLeft)}</span>
      </span>
    {/if}
    {#if turnsLeft != null}
      <span
        class:warn={turnsLeft <= 5}
        title="Turns left before the turn limit{when}, this one included; then the game is decided by score"
      >
        {turnsLeft === 1 ? 'Last turn' : `${turnsLeft} turns left`}
      </span>
    {/if}
  </div>
{/if}

<style>
  .limits {
    display: flex;
    flex-wrap: wrap;
    gap: 2px 12px;
    font-size: 12px;
    color: var(--muted);
  }
  .time {
    font-family: ui-monospace, 'DejaVu Sans Mono', monospace;
  }
  .warn {
    color: var(--clock-warn);
  }
</style>
