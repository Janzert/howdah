<script lang="ts">
  // A player's rating over time, for the player panel: the rating before
  // each rated game, then the rating now, as steps. A crosshair follows the pointer
  // to the nearest game and reads out its rating and date; a click opens
  // that game. The range buttons show the last year or five.
  import type { RatingPointView } from '../bindings/RatingPointView';

  interface Props {
    /** The rating before each rated game, oldest first. */
    points: RatingPointView[];
    /** The rating now, drawn after the last game. */
    current: number | null;
    /** Opens a game by its permanent id. */
    onOpen: (gid: string) => void;
    height?: number;
    label: string;
  }
  let { points, current, onOpen, height = 200, label }: Props = $props();

  let width = $state(600);
  const PAD = { top: 12, right: 16, bottom: 22, left: 44 };
  const DAY = 86_400;
  const YEAR = 365.25 * DAY;

  type Range = 'all' | '5y' | '1y';
  let range = $state<Range>('all');

  /** Each point, plus the rating now (no game) after the last one. */
  type Point = { time: number; rating: number; gid: string | null };
  const all = $derived.by((): Point[] => {
    const out: Point[] = points.map((p) => ({ time: p.time, rating: p.rating, gid: p.gid }));
    if (current != null && out.length > 0)
      out.push({ time: Math.max(Date.now() / 1000, out.at(-1)!.time), rating: current, gid: null });
    return out;
  });
  const end = $derived(all.at(-1)?.time ?? Date.now() / 1000);
  const span = $derived(all.length ? end - all[0].time : 0);
  /** The ranges worth offering: only those shorter than the history. */
  const ranges = $derived(
    (
      [
        ['5y', 5 * YEAR],
        ['1y', YEAR],
      ] as [Range, number][]
    ).filter(([, s]) => span > s * 1.2),
  );
  const start = $derived(range === '5y' ? end - 5 * YEAR : range === '1y' ? end - YEAR : (all[0]?.time ?? end - DAY));
  /** The points in range, with the one before it so the line enters from the edge. */
  const shown = $derived.by(() => {
    const i = all.findIndex((p) => p.time >= start);
    return i < 0 ? [] : all.slice(Math.max(0, i - 1));
  });

  const plotW = $derived(Math.max(40, width - PAD.left - PAD.right));
  const plotH = $derived(height - PAD.top - PAD.bottom);
  const domain = $derived.by((): [number, number] => {
    const rs = shown.filter((p) => p.time >= start).map((p) => p.rating);
    if (rs.length === 0) return [1400, 1600];
    let lo = Math.min(...rs);
    let hi = Math.max(...rs);
    const pad = Math.max(20, (hi - lo) * 0.08);
    return [lo - pad, hi + pad];
  });
  const xSpan = $derived(Math.max(end - start, DAY));
  const x = (t: number) => PAD.left + Math.max(0, (t - start) / xSpan) * plotW;
  const y = (r: number) => PAD.top + (1 - (r - domain[0]) / (domain[1] - domain[0])) * plotH;

  function ticks(lo: number, hi: number, count: number): number[] {
    const raw = (hi - lo) / count;
    const mag = 10 ** Math.floor(Math.log10(raw));
    const step = [1, 2, 5, 10].map((m) => m * mag).find((s) => s >= raw) ?? raw;
    const out = [];
    for (let t = Math.ceil(lo / step) * step; t <= hi; t += step) out.push(t);
    return out;
  }
  const yTicks = $derived(ticks(domain[0], domain[1], Math.max(2, Math.floor(plotH / 40))));

  /** Year marks, or month marks over a short range. */
  const xTicks = $derived.by((): { t: number; label: string }[] => {
    const room = Math.max(2, Math.floor(plotW / 70));
    const from = new Date(start * 1000);
    const years = xSpan / YEAR;
    const out: { t: number; label: string }[] = [];
    if (years > 1.5) {
      const step = [1, 2, 5, 10].find((s) => years / s <= room) ?? 10;
      for (let yr = Math.ceil(from.getUTCFullYear() / step) * step; ; yr += step) {
        const t = Date.UTC(yr, 0, 1) / 1000;
        if (t > end) break;
        if (t >= start) out.push({ t, label: String(yr) });
      }
    } else {
      const months = xSpan / (YEAR / 12);
      const step = [1, 2, 3, 6].find((s) => months / s <= room) ?? 6;
      let yr = from.getUTCFullYear();
      let m = Math.ceil((from.getUTCMonth() + 1) / step) * step;
      for (;;) {
        if (m >= 12) {
          yr += Math.floor(m / 12);
          m %= 12;
        }
        const t = Date.UTC(yr, m, 1) / 1000;
        if (t > end) break;
        out.push({ t, label: new Date(t * 1000).toLocaleDateString(undefined, { month: 'short', timeZone: 'UTC' }) });
        m += step;
      }
    }
    return out;
  });

  /** Steps: the rating holds between games and changes at each one (each
   * point's rating is the one before its game, so the change shows at the
   * game before it). */
  const path = $derived(
    shown
      .map((p, k) => {
        const py = y(p.rating).toFixed(1);
        if (k === 0) return `M${x(p.time).toFixed(1)},${py}`;
        return `L${x(shown[k - 1].time).toFixed(1)},${py}L${x(p.time).toFixed(1)},${py}`;
      })
      .join(''),
  );
  /** The highest rating in range. */
  const peak = $derived.by(() => {
    let best: Point | null = null;
    for (const p of shown) if (p.time >= start && (!best || p.rating > best.rating)) best = p;
    return best;
  });

  /** The point under the pointer. */
  let hover = $state<Point | null>(null);
  function move(e: PointerEvent) {
    const r = (e.currentTarget as SVGElement).getBoundingClientRect();
    const t = start + ((e.clientX - r.left - PAD.left) / plotW) * xSpan;
    let best: Point | null = null;
    for (const p of shown) {
      if (p.time < start) continue;
      if (!best || Math.abs(p.time - t) < Math.abs(best.time - t)) best = p;
    }
    hover = best;
  }
  const dateText = (t: number) =>
    new Date(t * 1000).toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric' });
</script>

<div class="rating-chart">
  {#if ranges.length > 0}
    <div class="ranges" role="group" aria-label="Range">
      <button class="link" class:on={range === 'all'} aria-pressed={range === 'all'} onclick={() => (range = 'all')}
        >All</button
      >
      {#each ranges as [r] (r)}
        <button class="link" class:on={range === r} aria-pressed={range === r} onclick={() => (range = r)}
          >{r === '5y' ? '5 years' : '1 year'}</button
        >
      {/each}
    </div>
  {/if}
  <div class="chart" bind:clientWidth={width}>
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
    <svg
      {width}
      {height}
      role="img"
      aria-label={label}
      class:clickable={hover?.gid != null}
      onpointermove={move}
      onpointerleave={() => (hover = null)}
      onclick={() => hover?.gid && onOpen(hover.gid)}
    >
      {#each yTicks as t (t)}
        <line class="grid" x1={PAD.left} x2={PAD.left + plotW} y1={y(t)} y2={y(t)} />
        <text class="tick" x={PAD.left - 6} y={y(t)} text-anchor="end" dominant-baseline="middle">{t}</text>
      {/each}
      {#each xTicks as t (t.t)}
        <text class="tick" x={x(t.t)} y={height - 6} text-anchor="middle">{t.label}</text>
      {/each}
      <line class="axis" x1={PAD.left} x2={PAD.left + plotW} y1={PAD.top + plotH} y2={PAD.top + plotH} />
      <clipPath id="rating-clip"><rect x={PAD.left} y={0} width={plotW} {height} /></clipPath>
      <path class="line" d={path} clip-path="url(#rating-clip)" />
      {#if peak}
        <circle class="peak" cx={x(peak.time)} cy={y(peak.rating)} r="3"><title>Highest: {peak.rating}</title></circle>
      {/if}
      {#if all.length > 0}
        {@const last = all.at(-1)!}
        <circle class="end" cx={x(last.time)} cy={y(last.rating)} r="4" />
      {/if}
      {#if hover}
        <line class="cross" x1={x(hover.time)} x2={x(hover.time)} y1={PAD.top} y2={PAD.top + plotH} />
        <circle class="end" cx={x(hover.time)} cy={y(hover.rating)} r="4" />
      {/if}
    </svg>
    {#if hover}
      <div class="readout" style="left: {Math.min(x(hover.time) + 8, width - 170)}px">
        <strong>{hover.rating}</strong>
        <span>{hover.gid ? `before game ${hover.gid}, ${dateText(hover.time)}` : 'now'}</span>
      </div>
    {/if}
  </div>
</div>

<style>
  .rating-chart {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .ranges {
    display: flex;
    gap: 10px;
    justify-content: flex-end;
    font-size: 12px;
  }
  .ranges .on {
    font-weight: 600;
    text-decoration: underline;
  }
  .chart {
    position: relative;
    width: 100%;
    user-select: none;
  }
  svg {
    display: block;
    overflow: visible;
  }
  svg.clickable {
    cursor: pointer;
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
  .tick {
    font-size: 11px;
    fill: var(--muted);
    font-variant-numeric: tabular-nums;
  }
  .line {
    fill: none;
    stroke: var(--accent);
    stroke-width: 1.5;
    stroke-linejoin: round;
  }
  .peak {
    fill: var(--clock-warn);
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
