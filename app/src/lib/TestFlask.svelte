<script lang="ts">
  // The flask icon for an engine test in the folded sidebar: its fluid
  // shows the SPRT's LLR by color (the warning color at the lower bound,
  // muted halfway, the ok color at the upper) and how far the test is
  // through its pair limit by level. A test without an SPRT has muted
  // fluid, and an SPSA run the accent color; one without a limit is filled
  // to the flask's usual line. The
  // outline keeps the sidebar's colors for the test's state.
  import type { TestView } from './bindings/TestView';
  import { llrPosition, testProgress } from './tests';

  let { test, size = 18 }: { test: TestView; size?: number } = $props();
  const uid = $props.id();

  /** The body's interior runs from the bottom (y 21) to the shoulder (9.5). */
  const BOTTOM = 21;
  const SHOULDER = 9.5;
  const OUTLINE = 'M10 3v6.5L4.8 18.2A1.9 1.9 0 0 0 6.4 21h11.2a1.9 1.9 0 0 0 1.6-2.8L14 9.5V3';

  /** A test that has started shows at least a film of fluid. */
  const level = $derived.by(() => {
    const p = testProgress(test) ?? (BOTTOM - 15) / (BOTTOM - SHOULDER);
    return test.games > 0 ? Math.max(p, 0.2) : p;
  });
  const top = $derived(BOTTOM - level * (BOTTOM - SHOULDER));
  const fluid = $derived.by(() => {
    const pos = llrPosition(test);
    if (test.spsa || test.pool) return 'var(--accent)';
    if (pos == null) return 'var(--muted)';
    return pos < 0.5
      ? `color-mix(in oklab, var(--muted) ${Math.round(pos * 200)}%, var(--warn))`
      : `color-mix(in oklab, var(--clock-ok) ${Math.round((pos - 0.5) * 200)}%, var(--muted))`;
  });
</script>

<svg width={size} height={size} viewBox="0 0 24 24" aria-hidden="true">
  <clipPath id="{uid}-inside"><path d="{OUTLINE}z" /></clipPath>
  {#if level > 0}
    <rect x="0" y={top} width="24" height={BOTTOM - top + 1} fill={fluid} clip-path="url(#{uid}-inside)" />
  {/if}
  <path
    d="M9 3h6{OUTLINE}"
    fill="none"
    stroke="currentColor"
    stroke-width="1.7"
    stroke-linecap="round"
    stroke-linejoin="round"
  />
</svg>

<style>
  svg {
    flex: none;
    display: block;
  }
</style>
