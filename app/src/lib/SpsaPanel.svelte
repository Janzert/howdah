<script lang="ts">
  // An SPSA tuning run's progress, in its test panel in place of the SPRT
  // results: iterations, pace and the end, a chart of θ over the run (the
  // parameters' mean movement from their starts, or one parameter's
  // value), and the parameters, each with where it stands in its range and
  // its path so far. θ's history comes from the daemon's
  // `spsa-history.jsonl`, fetched once and then only the steps added as
  // each iteration ends.
  import { onMount } from 'svelte';
  import { api, errorMessage } from './api';
  import type { SpsaParamView } from './bindings/SpsaParamView';
  import type { SpsaStepView } from './bindings/SpsaStepView';
  import type { SpsaView } from './bindings/SpsaView';
  import type { TestView } from './bindings/TestView';
  import { on } from './events';
  import SpsaChart from './SpsaChart.svelte';
  import {
    durationText,
    iterationsPerHour,
    meanMovement,
    moved,
    movedPastNoise,
    optionsText,
    orderParams,
    type ParamOrder,
    shareText,
    thin,
    valueText,
  } from './spsa';

  let { test }: { test: TestView & { spsa: SpsaView } } = $props();

  let steps = $state<SpsaStepView[]>([]);
  let error = $state<string | null>(null);
  /** The parameter charted (by index), or null for the mean movement. */
  let chosen = $state<number | null>(null);
  let order = $state<ParamOrder>('spec');
  let filter = $state('');
  let copied = $state(false);

  let fetching = false;
  /** Fetches the steps not yet here, if the run has taken more. */
  async function catchUp() {
    if (fetching) return;
    fetching = true;
    try {
      while (steps.length < test.spsa.done) {
        const more = await api.tourneySpsaHistory(test.id, steps.length);
        if (more.length === 0) break;
        steps = [...steps, ...more];
      }
      error = null;
    } catch (e) {
      error = errorMessage(e);
    }
    fetching = false;
  }

  onMount(() => {
    catchUp();
    const unlisten = on('tourney://changed', (u) => {
      if (u.test?.id === test.id && u.test.spsa && u.test.spsa.done > steps.length) catchUp();
    });
    return () => void unlisten.then((f) => f());
  });

  const spsa = $derived(test.spsa);
  const params = $derived(spsa.params);
  /** Each parameter's value after each step. */
  const columns = $derived(params.map((_, i) => steps.map((s) => s.theta[i])));
  const movement = $derived(meanMovement(params, steps));
  const shown = $derived(orderParams(params, order, filter));
  const pastNoise = $derived(params.filter(movedPastNoise).length);
  /** Whether the daemon sends each parameter's c (older ones don't). */
  const hasC = $derived(params.some((p) => p.c > 0));

  const perHour = $derived(iterationsPerHour(steps));
  const left = $derived(spsa.iterations - spsa.done);
  const endText = $derived.by(() => {
    if (test.status === 'finished' || left <= 0) return null;
    if (test.status !== 'active') return `${left} iterations left`;
    return perHour ? `about ${durationText(left / perHour)} left` : `${left} iterations left`;
  });
  /** The θ + cΔ side's score, which should stay near even. */
  const plusScore = $derived(test.games ? test.wins[0] / test.games : null);

  const chosenParam = $derived(chosen != null ? params[chosen] : null);

  async function copy() {
    try {
      await navigator.clipboard.writeText(optionsText(params));
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch (e) {
      error = errorMessage(e);
    }
  }

  /** Where `v` sits in a parameter's range, 0 to 1. */
  const at = (p: SpsaParamView, v: number) => Math.min(1, Math.max(0, (v - p.min) / (p.max - p.min)));

  const SPARK_W = 96;
  const SPARK_H = 22;
  /** A parameter's path as a sparkline, scaled to its own spread (with
   * its start and ± c around it, so noise looks like noise). */
  function spark(i: number): { d: string; startY: number } {
    const p = params[i];
    const xs = columns[i];
    let lo = Math.min(p.start - p.c, p.value);
    let hi = Math.max(p.start + p.c, p.value);
    for (const v of xs) {
      lo = Math.min(lo, v);
      hi = Math.max(hi, v);
    }
    const yOf = (v: number) => 2 + (1 - (v - lo) / (hi - lo || 1)) * (SPARK_H - 4);
    const n = Math.max(spsa.iterations, xs.length, 1);
    const d = thin(xs, 48)
      .map(([k, v], j) => `${j ? 'L' : 'M'}${(((k + 1) / n) * SPARK_W).toFixed(1)},${yOf(v).toFixed(1)}`)
      .join('');
    return { d: xs.length ? `M0,${yOf(p.start).toFixed(1)}L${d.slice(1)}` : '', startY: yOf(p.start) };
  }

  /** A parameter row's tooltip: its range, start, value and, when the
   * daemon sends it, the values the next iteration plays. */
  function rowTitle(p: SpsaParamView): string {
    const v = (x: number) => valueText(x, p.int);
    const lines = [
      `${p.name}${p.int ? ' (integer)' : ''}`,
      `range ${v(p.min)} to ${v(p.max)}, start ${v(p.start)}, now ${v(p.value)}`,
    ];
    if (p.c > 0) {
      lines.push(`c ${valueText(p.c)}: the next iteration plays ${v(p.value - p.c)} against ${v(p.value + p.c)}`);
    }
    return lines.join('\n');
  }

  function choose(i: number) {
    chosen = chosen === i ? null : i;
  }
</script>

<section class="spsa" aria-label="SPSA tuning">
  <div class="tiles">
    <div class="tile">
      <span class="label">Iterations</span>
      <span class="value">{spsa.done}<span class="of"> of {spsa.iterations}</span></span>
      <span class="bar" title="{((spsa.done / spsa.iterations) * 100).toFixed(1)}% of the iterations stepped"
        ><span style="width: {(spsa.done / spsa.iterations) * 100}%"></span></span
      >
      <span class="sub">{spsa.pairsPerIteration} pairs each</span>
    </div>
    <div class="tile">
      <span class="label">Pace</span>
      <span class="value">{perHour ? `${perHour < 10 ? perHour.toFixed(1) : Math.round(perHour)}` : '–'}</span>
      <span class="sub">{perHour ? 'iterations an hour' : 'not enough iterations yet'}</span>
      {#if endText}<span class="sub">{endText}</span>{/if}
    </div>
    <div
      class="tile"
      title="Games won by the side at θ + cΔ and the side at θ − cΔ. Directions are drawn at random, so this stays near even."
    >
      <span class="label">θ + cΔ against θ − cΔ</span>
      <span class="value">{plusScore != null ? `${(plusScore * 100).toFixed(1)}%` : '–'}</span>
      <span class="sub">{test.wins[0]}–{test.wins[1]} in {test.games} games</span>
    </div>
    {#if hasC}
      <div
        class="tile"
        title="Parameters that have moved from their start by more than the next iteration's perturbation (c)"
      >
        <span class="label">Moved more than c</span>
        <span class="value">{pastNoise}<span class="of"> of {params.length}</span></span>
        <span class="sub">mean move {shareText(movement.at(-1) ?? 0).replace('+', '')} of range</span>
      </div>
    {:else}
      <div class="tile">
        <span class="label">Mean move</span>
        <span class="value">{shareText(movement.at(-1) ?? 0).replace('+', '')}</span>
        <span class="sub">of each parameter's range</span>
      </div>
    {/if}
  </div>

  <div class="chart-card">
    <div class="chart-head">
      {#if chosenParam}
        <h3>{chosenParam.name}</h3>
        <span class="aside"
          >{valueText(chosenParam.start, chosenParam.int)} → {valueText(chosenParam.value, chosenParam.int)}
          ({shareText(moved(chosenParam))} of its range){chosenParam.c > 0
            ? `, c ${valueText(chosenParam.c)}`
            : ''}</span
        >
        <span class="spacer"></span>
        <button class="subtle" onclick={() => (chosen = null)}>Show all</button>
      {:else}
        <h3>Mean movement from the start</h3>
        <span class="aside">as a share of each parameter's range; pick a parameter below to chart it</span>
      {/if}
    </div>
    {#if steps.length === 0}
      <p class="note">
        {#if error}
          Couldn't load the run's history: {error}. A daemon older than this app doesn't serve it.
        {:else}
          {spsa.done === 0 ? 'No iteration has finished yet.' : 'Loading…'}
        {/if}
      </p>
    {:else if chosenParam && chosen != null}
      <SpsaChart
        label="{chosenParam.name} after each iteration"
        values={columns[chosen]}
        iterations={spsa.iterations}
        format={(v) => valueText(v, chosenParam.int)}
        marks={[
          { y: chosenParam.start, label: 'start', inView: true },
          { y: chosenParam.min, label: 'min' },
          { y: chosenParam.max, label: 'max' },
        ]}
        within={[chosenParam.start - chosenParam.c, chosenParam.start + chosenParam.c]}
        clamp={[
          chosenParam.min - (chosenParam.max - chosenParam.min) * 0.02,
          chosenParam.max + (chosenParam.max - chosenParam.min) * 0.02,
        ]}
      />
    {:else}
      <SpsaChart
        label="Mean movement of the parameters from their starts after each iteration"
        values={movement}
        iterations={spsa.iterations}
        format={(v) => `${+(v * 100).toFixed(2)}%`}
        within={[0, 0]}
      />
    {/if}
  </div>

  <div class="params-head">
    <h3>Parameters <span class="count">{params.length}</span></h3>
    <input type="search" placeholder="Filter" aria-label="Filter the parameters by name" bind:value={filter} />
    <select bind:value={order} aria-label="Order the parameters">
      <option value="spec">In the spec's order</option>
      <option value="moved">Furthest moved first</option>
      <option value="name">By name</option>
    </select>
    <span class="spacer"></span>
    <button
      class="subtle"
      onclick={copy}
      title="Copy θ as option lines, name = value, integers rounded as the engine gets them"
      >{copied ? 'Copied' : 'Copy values'}</button
    >
  </div>
  {#if error && steps.length > 0}<p class="error" role="alert">{error}</p>{/if}
  <div class="params" role="group" aria-label="Tuned parameters">
    <div class="param header" aria-hidden="true">
      <span>Name</span>
      <span>In its range</span>
      <span>So far</span>
      <span class="num">Now</span>
      <span class="num">Moved</span>
    </div>
    {#each shown as i (params[i].name)}
      {@const p = params[i]}
      {@const s = spark(i)}
      {@const m = moved(p)}
      <button
        class="param"
        class:on={chosen === i}
        onclick={() => choose(i)}
        aria-label="{p.name}: {valueText(p.value, p.int)}, started at {valueText(p.start, p.int)}"
        title={rowTitle(p)}
      >
        <span class="name">{p.name}</span>
        <span class="range">
          <span class="track"></span>
          <span
            class="band"
            style="left: {at(p, p.value - p.c) * 100}%; width: {(at(p, p.value + p.c) - at(p, p.value - p.c)) * 100}%"
          ></span>
          <span class="start" style="left: {at(p, p.start) * 100}%"></span>
          <span class="dot" style="left: {at(p, p.value) * 100}%"></span>
        </span>
        <svg class="spark" width={SPARK_W} height={SPARK_H} aria-hidden="true">
          <line x1="0" x2={SPARK_W} y1={s.startY} y2={s.startY} />
          <path d={s.d} />
        </svg>
        <span class="num">{valueText(p.value, p.int)}</span>
        <span class="num moved" class:quiet={!movedPastNoise(p)}>{shareText(m)}</span>
      </button>
    {:else}
      <p class="note">No parameter matches “{filter}”.</p>
    {/each}
  </div>
</section>

<style>
  .spsa {
    display: flex;
    flex-direction: column;
    gap: 12px;
    margin: 6px 0 4px;
  }
  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
    gap: 8px;
  }
  .tile {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: 8px;
    min-width: 0;
  }
  .label,
  .sub,
  .aside,
  .note,
  .count {
    font-size: 12px;
    color: var(--muted);
  }
  .value {
    font-size: 22px;
    font-weight: 600;
    line-height: 1.2;
  }
  .of {
    margin-left: 4px;
    font-size: 13px;
    font-weight: 400;
    color: var(--muted);
  }
  .bar {
    display: block;
    height: 6px;
    margin: 3px 0 1px;
    border-radius: 3px;
    background: color-mix(in oklab, var(--accent) 18%, transparent);
    overflow: hidden;
  }
  .bar span {
    display: block;
    height: 100%;
    border-radius: 3px;
    background: var(--accent);
  }
  .chart-card {
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 8px 10px 4px;
  }
  .chart-head,
  .params-head {
    display: flex;
    align-items: baseline;
    flex-wrap: wrap;
    gap: 4px 8px;
    margin-bottom: 4px;
  }
  .params-head {
    align-items: center;
  }
  h3 {
    margin: 0;
    font-size: 14px;
  }
  .chart-head h3 {
    overflow-wrap: anywhere;
  }
  .params-head input {
    width: 12em;
  }
  .params {
    display: flex;
    flex-direction: column;
    max-height: 60vh;
    overflow-y: auto;
    border: 1px solid var(--border);
    border-radius: 8px;
  }
  .param {
    display: grid;
    grid-template-columns: minmax(8em, 1fr) minmax(90px, 160px) 96px 5.5em 4.5em;
    align-items: center;
    gap: 10px;
    padding: 3px 10px;
    border: none;
    border-radius: 0;
    background: none;
    color: inherit;
    font: inherit;
    font-size: 13px;
    text-align: left;
    cursor: pointer;
  }
  .param:hover {
    background: var(--hover);
  }
  .param.on {
    background: color-mix(in oklab, var(--accent) 14%, transparent);
  }
  .param.header {
    position: sticky;
    top: 0;
    z-index: 1;
    background: var(--panel);
    border-bottom: 1px solid var(--border);
    font-size: 11px;
    color: var(--muted);
    cursor: default;
  }
  .name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .moved.quiet {
    color: var(--muted);
  }
  .range {
    position: relative;
    height: 14px;
  }
  .track {
    position: absolute;
    top: 6px;
    left: 0;
    right: 0;
    height: 2px;
    border-radius: 1px;
    background: var(--border);
  }
  .band {
    position: absolute;
    top: 3px;
    height: 8px;
    border-radius: 4px;
    background: color-mix(in oklab, var(--accent) 22%, transparent);
  }
  .start {
    position: absolute;
    top: 1px;
    width: 1px;
    height: 12px;
    background: var(--muted);
  }
  .dot {
    position: absolute;
    top: 3px;
    width: 8px;
    height: 8px;
    margin-left: -4px;
    border-radius: 50%;
    background: var(--accent);
    box-shadow: 0 0 0 2px var(--panel);
  }
  .spark line {
    stroke: var(--border);
    stroke-width: 1;
  }
  .spark path {
    fill: none;
    stroke: var(--accent);
    stroke-width: 1.5;
    stroke-linejoin: round;
  }
  .error {
    color: var(--warn);
    font-size: 13px;
    margin: 0;
  }
  .spacer {
    flex: 1;
  }
  .subtle {
    font-size: 12px;
    padding: 2px 8px;
    color: var(--muted);
  }
  .note {
    margin: 8px 0;
  }
  @media (max-width: 560px) {
    .param {
      grid-template-columns: minmax(6em, 1fr) 80px 4.5em;
    }
    .param .spark,
    .param .moved,
    .param.header span:nth-child(3),
    .param.header span:nth-child(5) {
      display: none;
    }
  }
</style>
