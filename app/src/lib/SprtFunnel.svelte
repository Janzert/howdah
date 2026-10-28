<script lang="ts">
  // The SPRT funnel for the test panel (`lib/sprtFunnel.ts` works it
  // out): the observed Elo across, the games down from the first at the
  // top to the pair limit at the bottom, the fail curve at the left and
  // the pass curve at the right closing in like a funnel, and the test's
  // observed Elo after each pair running down between them. `mini` is the
  // panel's small one (curves, elo0 and elo1, the path); the full one adds
  // axes, a legend, the band where the test stops undecided, the path's
  // 95% interval (from the tenth pair; before that it spans everything),
  // where the test is likely to end, and a readout for the
  // game count under the pointer.
  import type { TestGameView } from './bindings/TestGameView';
  import type { TestView } from './bindings/TestView';
  import {
    cutoffsAt,
    eloSpan,
    funnelCurves,
    funnelOf,
    funnelPath,
    pairsShown,
    thinPath,
    undecidedBand,
    type CurvePoint,
  } from './sprtFunnel';

  interface Props {
    test: TestView;
    /** The test's finished games, newest first. */
    games: TestGameView[];
    mini?: boolean;
    /** The mini one's width; the full one fills its container. */
    width?: number;
    height?: number;
  }
  let { test, games, mini = false, width: miniWidth = 132, height = mini ? 120 : 480 }: Props = $props();
  const uid = $props.id();

  let boxWidth = $state(800);
  const width = $derived(mini ? miniWidth : boxWidth);
  const PAD = $derived(mini ? { top: 3, right: 3, bottom: 3, left: 3 } : { top: 10, right: 18, bottom: 40, left: 62 });
  const plotW = $derived(Math.max(40, width - PAD.left - PAD.right));
  const plotH = $derived(Math.max(40, height - PAD.top - PAD.bottom));

  const f = $derived(funnelOf(test));
  const shown = $derived(pairsShown(test));
  /** The curves change only with the bounds and the pairs shown. */
  const curves = $derived(f ? funnelCurves(f, shown, mini ? 150 : 400) : { fail: [], pass: [] });
  const path = $derived(funnelPath(games));
  const centre = $derived(f ? (f.elo0 + f.elo1) / 2 : 0);
  const half = $derived(f ? eloSpan(f, path, shown) : 100);
  const band = $derived(
    !mini && f && test.maxPairs != null && test.sprt?.undecidedBelow != null
      ? undecidedBand(f, test.maxPairs, test.sprt.undecidedBelow)
      : [],
  );

  /** Far enough off the plot to be clipped, finite for the path's sake. */
  const clampElo = (e: number) => Math.max(centre - 3 * half, Math.min(centre + 3 * half, e));
  const x = (e: number) => PAD.left + ((clampElo(e) - (centre - half)) / (2 * half)) * plotW;
  const y = (pairs: number) => PAD.top + (pairs / shown) * plotH;
  const line = (pts: CurvePoint[]) =>
    pts.map(([n, e], i) => `${i ? 'L' : 'M'}${x(e).toFixed(1)},${y(n).toFixed(1)}`).join('');

  const failPath = $derived(line(curves.fail));
  const passPath = $derived(line(curves.pass));
  /** The fail region left of its curve, the pass region right of its. */
  const failArea = $derived.by(() => {
    const c = curves.fail;
    if (!c.length) return '';
    return `${failPath}L${PAD.left - 2},${y(c[c.length - 1][0])}L${PAD.left - 2},${y(c[0][0])}Z`;
  });
  const passArea = $derived.by(() => {
    const c = curves.pass;
    if (!c.length) return '';
    const edge = PAD.left + plotW + 2;
    return `${passPath}L${edge},${y(c[c.length - 1][0])}L${edge},${y(c[0][0])}Z`;
  });
  const bandArea = $derived(
    band.length
      ? `${line(band.map(([n, lo]) => [n, lo]))}${line(band.map(([n, , hi]): CurvePoint => [n, hi]).reverse()).replace('M', 'L')}Z`
      : '',
  );

  const thinned = $derived(thinPath(path, mini ? 200 : 1500));
  const pathLine = $derived(line(thinned.map((p): CurvePoint => [p.pairs, p.elo])));
  const intervalArea = $derived.by(() => {
    if (mini) return '';
    const pts = thinned.filter((p) => p.interval && p.pairs >= 10);
    if (pts.length < 2) return '';
    const lo = line(pts.map((p): CurvePoint => [p.pairs, p.interval![0]]));
    const hi = line(pts.map((p): CurvePoint => [p.pairs, p.interval![1]]).reverse());
    return `${lo}${hi.replace('M', 'L')}Z`;
  });
  const last = $derived(path.length ? path[path.length - 1] : null);

  /** Where an unfinished test is likely to end, in pairs: the 80% range
   * and the median. */
  const ends = $derived.by(() => {
    const p = test.projection;
    if (mini || !p || test.status === 'finished') return null;
    const at = (left: number) => Math.min(shown, (test.games + left) / 2);
    return { from: at(p.gamesLeft[0]), median: at(p.gamesLeft[1]), to: at(p.gamesLeft[2]) };
  });

  /** Round tick values: 1, 2 or 5 times a power of ten apart. */
  function ticks(lo: number, hi: number, count: number): number[] {
    const raw = (hi - lo) / Math.max(1, count);
    const mag = 10 ** Math.floor(Math.log10(raw));
    const step = [1, 2, 5, 10].map((m) => m * mag).find((s) => s >= raw) ?? raw;
    const out = [];
    for (let t = Math.ceil(lo / step) * step; t <= hi + step * 1e-9; t += step) out.push(+t.toPrecision(12));
    return out;
  }
  const eloTicks = $derived(mini ? [] : ticks(centre - half, centre + half, Math.max(3, Math.floor(plotW / 80))));
  const gameTicks = $derived(mini ? [] : ticks(0, 2 * shown, Math.max(3, Math.floor(plotH / 50))));
  const signed = (e: number, digits = 0) => {
    if (!Number.isFinite(e)) return e > 0 ? '+∞' : '−∞';
    const r = +e.toFixed(digits);
    return `${r > 0 ? '+' : r < 0 ? '−' : ''}${Math.abs(r).toFixed(digits)}`;
  };

  /** The pair count under the pointer, or null. */
  let hover = $state<number | null>(null);
  function move(e: PointerEvent) {
    const r = (e.currentTarget as SVGElement).getBoundingClientRect();
    const n = Math.round(((e.clientY - r.top - PAD.top) / plotH) * shown);
    hover = n >= 1 && n <= shown ? n : null;
  }
  const readout = $derived.by(() => {
    if (hover == null || !f) return null;
    const c = cutoffsAt(f, hover);
    return { n: hover, ...c, at: path[hover - 1] ?? null };
  });
</script>

{#if f}
  <div class="funnel" class:mini bind:clientWidth={boxWidth}>
    {#if !mini}
      <div class="legend">
        <span><i class="swatch" style="background: var(--warn)"></i>Fails at or below</span>
        <span><i class="swatch" style="background: var(--clock-ok)"></i>Passes at or above</span>
        <span><i class="swatch dash"></i>elo0, elo1</span>
        {#if band.length}<span><i class="swatch zone"></i>Stops undecided</span>{/if}
        <span><i class="swatch" style="background: var(--accent)"></i>Observed Elo, with its 95% interval</span>
        {#if ends}<span><i class="swatch ends"></i>Likely end (80%)</span>{/if}
      </div>
    {/if}
    <div class="plot">
      <svg
        {width}
        {height}
        role="img"
        aria-label="SPRT funnel: the observed Elo after each game against the Elo at which the test passes or fails"
        onpointermove={mini ? undefined : move}
        onpointerleave={() => (hover = null)}
      >
        <clipPath id="{uid}-plot"><rect x={PAD.left} y={PAD.top} width={plotW} height={plotH} /></clipPath>
        {#each eloTicks as t (t)}
          <line class="grid" x1={x(t)} x2={x(t)} y1={PAD.top} y2={PAD.top + plotH} />
          <text class="tick" x={x(t)} y={PAD.top + plotH + 16} text-anchor="middle">{signed(t)}</text>
        {/each}
        {#each gameTicks as t (t)}
          <line class="grid" x1={PAD.left} x2={PAD.left + plotW} y1={y(t / 2)} y2={y(t / 2)} />
          <text class="tick" x={PAD.left - 6} y={y(t / 2)} text-anchor="end" dominant-baseline="middle"
            >{t.toLocaleString('en-US')}</text
          >
        {/each}
        {#if !mini}
          <text class="title" x={PAD.left + plotW / 2} y={height - 4} text-anchor="middle">Observed Elo</text>
          <text
            class="title"
            x={14}
            y={PAD.top + plotH / 2}
            text-anchor="middle"
            transform="rotate(-90 14 {PAD.top + plotH / 2})">Games</text
          >
        {/if}
        <g clip-path="url(#{uid}-plot)">
          <path class="area fail" d={failArea} />
          <path class="area pass" d={passArea} />
          {#if bandArea}<path class="zone" d={bandArea} />{/if}
          {#if ends}
            <rect
              class="ends"
              x={PAD.left}
              y={y(ends.from)}
              width={plotW}
              height={Math.max(1, y(ends.to) - y(ends.from))}
            />
            <line class="ends-median" x1={PAD.left} x2={PAD.left + plotW} y1={y(ends.median)} y2={y(ends.median)} />
          {/if}
          {#each [f.elo0, f.elo1] as e, i (i)}
            <line class="bound" x1={x(e)} x2={x(e)} y1={PAD.top} y2={PAD.top + plotH} />
          {/each}
          <path class="curve fail" d={failPath} />
          <path class="curve pass" d={passPath} />
          {#if intervalArea}<path class="interval" d={intervalArea} />{/if}
          <path class="observed" d={pathLine} />
          {#if last}
            <circle class="now" cx={x(last.elo)} cy={y(last.pairs)} r={mini ? 3 : 4} />
          {/if}
          {#if readout}
            <line class="cross" x1={PAD.left} x2={PAD.left + plotW} y1={y(readout.n)} y2={y(readout.n)} />
            {#if readout.fail != null}<circle class="dot fail" cx={x(readout.fail)} cy={y(readout.n)} r="4" />{/if}
            {#if readout.pass != null}<circle class="dot pass" cx={x(readout.pass)} cy={y(readout.n)} r="4" />{/if}
            {#if readout.at}<circle class="now" cx={x(readout.at.elo)} cy={y(readout.n)} r="4" />{/if}
          {/if}
        </g>
        {#if !mini}
          <line class="axis" x1={PAD.left} x2={PAD.left} y1={PAD.top} y2={PAD.top + plotH} />
        {/if}
      </svg>
      {#if readout}
        <div
          class="readout"
          style="{y(readout.n) > height - 100
            ? `bottom: ${height - y(readout.n) + 8}px`
            : `top: ${y(readout.n) + 8}px`}; left: {PAD.left + 8}px"
        >
          <strong>After {(2 * readout.n).toLocaleString('en-US')} games</strong>
          <span>Passes at {readout.pass == null ? '—' : `${signed(readout.pass, 1)} or above`}</span>
          <span>Fails at {readout.fail == null ? '—' : `${signed(readout.fail, 1)} or below`}</span>
          {#if readout.at}
            <span
              >Observed {signed(readout.at.elo, 1)}{readout.at.interval
                ? ` [${signed(readout.at.interval[0], 1)}, ${signed(readout.at.interval[1], 1)}]`
                : ''}</span
            >
          {/if}
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .funnel {
    position: relative;
    width: 100%;
    user-select: none;
  }
  .funnel.mini {
    width: auto;
  }
  .plot {
    position: relative;
  }
  svg {
    display: block;
  }
  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 16px;
    margin: 0 0 6px;
    font-size: 12px;
    color: var(--muted);
  }
  .legend span {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .swatch {
    display: inline-block;
    width: 16px;
    height: 2px;
    border-radius: 1px;
  }
  .swatch.dash {
    height: 0;
    border-top: 1.5px dashed var(--muted);
  }
  .swatch.zone {
    height: 10px;
    background: color-mix(in srgb, var(--muted) 18%, transparent);
    border: 1px dashed var(--muted);
  }
  .swatch.ends {
    height: 10px;
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    border-bottom: 1.5px solid color-mix(in srgb, var(--accent) 60%, transparent);
  }
  .grid {
    stroke: var(--border);
    stroke-width: 1;
    opacity: 0.6;
  }
  .axis {
    stroke: var(--border);
  }
  .tick {
    font-size: 11px;
    fill: var(--muted);
    font-variant-numeric: tabular-nums;
  }
  .title {
    font-size: 12px;
    fill: var(--text);
  }
  .area.fail {
    fill: color-mix(in srgb, var(--warn) 10%, transparent);
  }
  .area.pass {
    fill: color-mix(in srgb, var(--clock-ok) 10%, transparent);
  }
  .zone {
    fill: color-mix(in srgb, var(--muted) 18%, transparent);
    stroke: var(--muted);
    stroke-width: 1.25;
    stroke-dasharray: 4 3;
  }
  .ends {
    fill: color-mix(in srgb, var(--accent) 12%, transparent);
  }
  .ends-median {
    stroke: color-mix(in srgb, var(--accent) 60%, transparent);
    stroke-width: 1.5;
  }
  .bound {
    stroke: var(--muted);
    stroke-width: 1;
    stroke-dasharray: 4 4;
  }
  .mini .bound {
    stroke-dasharray: 2 3;
  }
  .curve {
    fill: none;
    stroke-width: 2;
    stroke-linejoin: round;
  }
  .mini .curve {
    stroke-width: 1.5;
  }
  .curve.fail,
  .dot.fail {
    stroke: var(--warn);
  }
  .curve.pass,
  .dot.pass {
    stroke: var(--clock-ok);
  }
  .dot.fail {
    fill: var(--warn);
  }
  .dot.pass {
    fill: var(--clock-ok);
  }
  .dot {
    stroke: var(--panel);
    stroke-width: 2;
  }
  .interval {
    fill: color-mix(in srgb, var(--accent) 16%, transparent);
  }
  .observed {
    fill: none;
    stroke: var(--accent);
    stroke-width: 1.75;
    stroke-linejoin: round;
  }
  .mini .observed {
    stroke-width: 1.25;
  }
  .now {
    fill: var(--accent);
    stroke: var(--panel);
    stroke-width: 1.5;
  }
  .cross {
    stroke: var(--muted);
    stroke-width: 1;
  }
  .readout {
    position: absolute;
    display: flex;
    flex-direction: column;
    padding: 4px 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--panel);
    font-size: 12px;
    pointer-events: none;
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }
  .readout span {
    color: var(--muted);
  }
</style>
