<script lang="ts">
  // A line chart of one series over an SPSA run's iterations, for the
  // test panel: the x axis runs to the run's last iteration, so the line
  // grows across as the run goes. Reference lines (a parameter's start,
  // its bounds) are dashed-free hairlines with a label at the right. A
  // crosshair follows the pointer and reads out the iteration and value.
  import { thin } from './spsa';

  interface Props {
    /** The value after each step, in the order taken. */
    values: number[];
    /** The run's iterations, the x axis's end. */
    iterations: number;
    /** How a value reads in the axis and the readout. */
    format: (v: number) => string;
    /** Horizontal lines to mark, each with a short label; `inView` ones
     * are kept in view, the others drawn only when the values reach them. */
    marks?: { y: number; label: string; inView?: boolean }[];
    /** A band to keep in view (with the values and marks). */
    within?: [number, number];
    /** Bounds the view never goes past. */
    clamp?: [number, number];
    height?: number;
    /** Names the chart for screen readers. */
    label: string;
  }
  let { values, iterations, format, marks = [], within, clamp, height = 180, label }: Props = $props();

  let width = $state(600);
  const PAD = { top: 10, right: 56, bottom: 22, left: 52 };
  /** Points drawn at most, so long runs stay light. */
  const MAX_POINTS = 600;

  const plotW = $derived(Math.max(40, width - PAD.left - PAD.right));
  const plotH = $derived(height - PAD.top - PAD.bottom);

  const domain = $derived.by((): [number, number] => {
    const kept = [...values.filter(Number.isFinite), ...marks.filter((m) => m.inView).map((m) => m.y)];
    if (within) kept.push(...within);
    if (kept.length === 0) return [0, 1];
    let lo = Math.min(...kept);
    let hi = Math.max(...kept);
    if (hi === lo) {
      lo -= 1;
      hi += 1;
    }
    const pad = (hi - lo) * 0.06;
    lo -= pad;
    hi += pad;
    if (clamp) {
      lo = Math.max(lo, clamp[0]);
      hi = Math.min(hi, clamp[1]);
    }
    return [lo, hi];
  });

  const xEnd = $derived(Math.max(iterations, values.length, 1));
  const x = (i: number) => PAD.left + (i / xEnd) * plotW;
  const y = (v: number) => PAD.top + (1 - (v - domain[0]) / (domain[1] - domain[0])) * plotH;

  /** Round tick values: 1, 2 or 5 times a power of ten apart. */
  function ticks(lo: number, hi: number, count: number): number[] {
    const raw = (hi - lo) / count;
    const mag = 10 ** Math.floor(Math.log10(raw));
    const step = [1, 2, 5, 10].map((m) => m * mag).find((s) => s >= raw) ?? raw;
    const out = [];
    for (let t = Math.ceil(lo / step) * step; t <= hi + step * 1e-9; t += step) out.push(+t.toPrecision(12));
    return out;
  }
  const yTicks = $derived(ticks(domain[0], domain[1], Math.max(2, Math.floor(plotH / 40))));
  const xTicks = $derived(ticks(0, xEnd, Math.max(2, Math.floor(plotW / 90))).filter((t) => t <= xEnd));

  const path = $derived(
    thin(values, MAX_POINTS)
      .filter(([, v]) => Number.isFinite(v))
      .map(([i, v], k) => `${k ? 'L' : 'M'}${x(i + 1).toFixed(1)},${y(v).toFixed(1)}`)
      .join(''),
  );
  const last = $derived(values.length ? values[values.length - 1] : null);

  /** The step under the pointer (from 0), or null. */
  let hover = $state<number | null>(null);
  function move(e: PointerEvent) {
    const r = (e.currentTarget as SVGElement).getBoundingClientRect();
    const i = Math.round(((e.clientX - r.left - PAD.left) / plotW) * xEnd) - 1;
    hover = values.length ? Math.max(0, Math.min(values.length - 1, i)) : null;
  }
</script>

<div class="chart" bind:clientWidth={width}>
  <svg {width} {height} role="img" aria-label={label} onpointermove={move} onpointerleave={() => (hover = null)}>
    {#each yTicks as t (t)}
      <line class="grid" x1={PAD.left} x2={PAD.left + plotW} y1={y(t)} y2={y(t)} />
      <text class="tick" x={PAD.left - 6} y={y(t)} text-anchor="end" dominant-baseline="middle">{format(t)}</text>
    {/each}
    {#each xTicks as t (t)}
      <text class="tick" x={x(t)} y={height - 6} text-anchor="middle">{t}</text>
    {/each}
    <line class="axis" x1={PAD.left} x2={PAD.left + plotW} y1={PAD.top + plotH} y2={PAD.top + plotH} />
    {#each marks as m (m.label)}
      {#if m.y >= domain[0] && m.y <= domain[1]}
        <line class="mark" x1={PAD.left} x2={PAD.left + plotW} y1={y(m.y)} y2={y(m.y)} />
        <text class="mark-label" x={PAD.left + plotW + 4} y={y(m.y)} dominant-baseline="middle">{m.label}</text>
      {/if}
    {/each}
    <path class="line" d={path} />
    {#if last != null && Number.isFinite(last)}
      <circle class="end" cx={x(values.length)} cy={y(last)} r="4" />
    {/if}
    {#if hover != null}
      {@const v = values[hover]}
      <line class="cross" x1={x(hover + 1)} x2={x(hover + 1)} y1={PAD.top} y2={PAD.top + plotH} />
      {#if Number.isFinite(v)}<circle class="end" cx={x(hover + 1)} cy={y(v)} r="4" />{/if}
    {/if}
  </svg>
  {#if hover != null}
    {@const left = x(hover + 1)}
    <div class="readout" style="left: {Math.min(left + 8, width - 150)}px">
      <strong>{format(values[hover])}</strong>
      <span>after iteration {hover + 1}</span>
    </div>
  {/if}
</div>

<style>
  .chart {
    position: relative;
    width: 100%;
    user-select: none;
  }
  svg {
    display: block;
    overflow: visible;
  }
  .grid {
    stroke: var(--border);
    stroke-width: 1;
    opacity: 0.6;
  }
  .axis {
    stroke: var(--border);
    stroke-width: 1;
  }
  .tick,
  .mark-label {
    font-size: 11px;
    fill: var(--muted);
    font-variant-numeric: tabular-nums;
  }
  .mark {
    stroke: var(--muted);
    stroke-width: 1;
  }
  .line {
    fill: none;
    stroke: var(--accent);
    stroke-width: 2;
    stroke-linejoin: round;
    stroke-linecap: round;
  }
  .end {
    fill: var(--accent);
    stroke: var(--panel);
    stroke-width: 2;
  }
  .cross {
    stroke: var(--muted);
    stroke-width: 1;
  }
  .readout {
    position: absolute;
    top: 4px;
    display: flex;
    flex-direction: column;
    padding: 4px 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--panel);
    font-size: 12px;
    pointer-events: none;
    white-space: nowrap;
  }
  .readout span {
    color: var(--muted);
  }
</style>
