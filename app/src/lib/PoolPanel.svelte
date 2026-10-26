<script lang="ts">
  // A pool's standings, in place of a test's results in `TestPanel`: the
  // ratings in rank order with their 95% intervals (Elo from the anchor,
  // drawn as a dot and whisker), the crosstable in the same order, each
  // cell shaded by how far its result is from what the ratings predict,
  // and the outliers in words. Engines from the engine list can be added
  // while it runs (they play their matchups first), and retired (no new
  // pairs; their games still count) or brought back.
  import { onMount } from 'svelte';
  import { api, errorMessage } from './api';
  import type { EngineSpec } from './bindings/EngineSpec';
  import type { PoolView } from './bindings/PoolView';
  import type { TestView } from './bindings/TestView';
  import { eloText, matchupFor, zShade } from './tests';

  interface Props {
    test: TestView & { pool: PoolView };
    /** The daemon changed the test (an engine added or retired). */
    onChanged: (t: TestView) => void;
  }
  let { test, onChanged }: Props = $props();

  const pool = $derived(test.pool);
  let engines = $state<EngineSpec[]>([]);
  let adding = $state('');
  let busy = $state(false);
  let error = $state<string | null>(null);

  onMount(() => {
    api
      .listEngines()
      .then((list) => {
        engines = list;
        adding = list[0]?.id ?? '';
      })
      .catch((e) => (error = errorMessage(e)));
  });

  async function act(f: () => Promise<TestView>) {
    busy = true;
    error = null;
    try {
      onChanged(await f());
    } catch (e) {
      error = errorMessage(e);
    }
    busy = false;
  }

  /** The intervals' scale: every rated engine's interval, with a margin. */
  const scale = $derived.by(() => {
    const rated = pool.engines.filter((e) => e.elo != null);
    let lo = Math.min(0, ...rated.map((e) => (e.elo ?? 0) - (e.error ?? 0)));
    let hi = Math.max(0, ...rated.map((e) => (e.elo ?? 0) + (e.error ?? 0)));
    if (hi - lo < 50) [lo, hi] = [(lo + hi) / 2 - 25, (lo + hi) / 2 + 25];
    const pad = (hi - lo) * 0.04;
    return { lo: lo - pad, hi: hi + pad };
  });
  const PLOT_W = 180;
  const x = (elo: number) => ((elo - scale.lo) / (scale.hi - scale.lo)) * PLOT_W;

  const outliers = $derived(pool.matchups.filter((m) => m.outlier));
  const anchorName = $derived(pool.anchor != null ? pool.engines[pool.anchor].name : null);
  const pct = (n: number, of: number) => (of > 0 ? `${((100 * n) / of).toFixed(1)}%` : '–');
  const name = (i: number) => pool.engines[i].name;

  /** A cell's tooltip: the result, what the ratings expect, and how far
   * apart they are. */
  function cellTitle(i: number, j: number): string {
    const m = matchupFor(pool, i, j);
    if (!m) return `${name(i)} and ${name(j)} haven't played yet`;
    const n = m.wins + m.losses;
    const pairs = m.pairs === 1 ? '1 pair' : `${m.pairs} pairs`;
    const side = m.z >= 0 ? 'better' : 'worse';
    return (
      `${name(i)} against ${name(j)}: ${m.wins}–${m.losses} in ${n} games (${pairs}).\n` +
      `The ratings expect ${(m.expected * n).toFixed(1)}–${((1 - m.expected) * n).toFixed(1)}: ` +
      `this is ${Math.abs(m.z).toFixed(1)} standard deviations ${side}` +
      (m.outlier ? ', out of line.' : '.')
    );
  }
</script>

<section class="pool" aria-label="Pool standings">
  {#if pool.anchor == null}
    <p class="note">No games yet. Every pair of engines plays {pool.minPairs} pairs first.</p>
  {/if}
  <table class="ratings">
    <caption class="note">
      {#if anchorName}Elo relative to {anchorName}, with 95% intervals, from all the games{/if}
    </caption>
    <thead>
      <tr>
        <th class="num">#</th>
        <th>Engine</th>
        <th class="num">Elo</th>
        <th aria-label="Interval"></th>
        <th class="num">Games</th>
        <th class="num">Won</th>
        <th></th>
      </tr>
    </thead>
    <tbody>
      {#each pool.ranking as i, rank (i)}
        {@const e = pool.engines[i]}
        <tr class:retired={e.retired}>
          <td class="num">{rank + 1}</td>
          <td class="name"
            >{e.name}{#if e.retired}<span class="tag">retired</span>{/if}</td
          >
          <td class="num">{eloText(e)}</td>
          <td class="plot">
            {#if e.elo != null}
              <svg width={PLOT_W} height="14" role="img" aria-label="{e.name}: {eloText(e)}">
                <title>{e.name}: {eloText(e)}</title>
                <line class="zero" x1={x(0)} x2={x(0)} y1="0" y2="14" />
                {#if e.error}
                  <line class="whisker" x1={x(e.elo - e.error)} x2={x(e.elo + e.error)} y1="7" y2="7" />
                {/if}
                <circle cx={x(e.elo)} cy="7" r="4" />
              </svg>
            {/if}
          </td>
          <td class="num">{e.games}</td>
          <td class="num">{pct(e.wins, e.games)}</td>
          <td>
            <button
              class="subtle"
              disabled={busy}
              onclick={() => act(() => api.tourneySetEngineRetired(test.id, e.name, !e.retired))}
              title={e.retired ? 'Play new pairs with it again' : 'Play no new pairs with it; its games still count'}
              >{e.retired ? 'Bring back' : 'Retire'}</button
            >
          </td>
        </tr>
      {/each}
    </tbody>
  </table>

  <div class="row">
    <label for="pool-add">Add an engine</label>
    <select id="pool-add" bind:value={adding} disabled={engines.length === 0}>
      {#each engines as e (e.id)}<option value={e.id}>{e.name}</option>{/each}
    </select>
    <button
      disabled={busy || !adding}
      onclick={() => act(() => api.tourneyAddEngine(test.id, adding))}
      title="Copy it into the pool; it plays {pool.minPairs} pairs against each engine first, then joins the rest"
      >Add</button
    >
  </div>
  {#if error}<p class="error" role="alert">{error}</p>{/if}

  {#if pool.matchups.length > 0}
    <h3>Crosstable</h3>
    <div class="scroll">
      <table class="cross">
        <caption class="note">Each row's wins–losses against each column, both in rank order</caption>
        <thead>
          <tr>
            <th></th>
            {#each pool.ranking as j, rank (j)}<th class="num" title={name(j)}>{rank + 1}</th>{/each}
          </tr>
        </thead>
        <tbody>
          {#each pool.ranking as i, rank (i)}
            <tr>
              <th class="name" scope="row">{rank + 1}. {name(i)}</th>
              {#each pool.ranking as j (j)}
                {#if i === j}
                  <td class="self">·</td>
                {:else}
                  {@const m = matchupFor(pool, i, j)}
                  <td
                    class:outlier={m?.outlier}
                    style:background={m ? zShade(m.z, pool.outlierZ) : null}
                    title={cellTitle(i, j)}>{m ? `${m.wins}–${m.losses}` : ''}{m?.outlier ? '*' : ''}</td
                  >
                {/if}
              {/each}
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
    <p class="legend note">
      <span class="swatch" style:background={zShade(pool.outlierZ, pool.outlierZ)}></span> better than the ratings
      predict
      <span class="swatch" style:background={zShade(-pool.outlierZ, pool.outlierZ)}></span> worse
      <span class="mark">*</span> out of line ({pool.outlierZ} standard deviations or more)
    </p>
  {/if}

  {#if outliers.length > 0}
    <h3>Out of line</h3>
    <ul class="outliers">
      {#each outliers as m (`${m.a}-${m.b}`)}
        {@const n = m.wins[0] + m.wins[1]}
        <li>
          <strong>{name(m.a)}</strong>
          {m.wins[0]}–{m.wins[1]}
          <strong>{name(m.b)}</strong>: the ratings expect {(m.expected * n).toFixed(1)}–{(
            (1 - m.expected) *
            n
          ).toFixed(1)}
          <span class="note">(z {m.z >= 0 ? '+' : ''}{m.z.toFixed(1)})</span>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .pool {
    margin: 6px 0;
    font-size: 13px;
  }
  .note {
    margin: 0 0 6px;
    font-size: 12px;
    color: var(--muted);
  }
  caption {
    text-align: left;
  }
  table {
    border-collapse: collapse;
  }
  th {
    font-weight: 500;
    color: var(--muted);
    text-align: left;
  }
  .ratings td,
  .ratings th {
    padding: 2px 8px 2px 0;
  }
  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .name {
    max-width: 16em;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .retired td {
    color: var(--muted);
  }
  .tag {
    margin-left: 6px;
    font-size: 11px;
    padding: 0 5px;
    border-radius: 8px;
    border: 1px solid var(--border);
  }
  .plot svg {
    display: block;
    overflow: visible;
  }
  .zero {
    stroke: var(--border);
    stroke-width: 1;
  }
  .whisker {
    stroke: var(--muted);
    stroke-width: 2;
    stroke-linecap: round;
  }
  circle {
    fill: var(--accent);
    stroke: var(--panel, var(--bg));
    stroke-width: 2;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 8px 0;
  }
  h3 {
    margin: 12px 0 4px;
    font-size: 14px;
  }
  .scroll {
    overflow-x: auto;
  }
  .cross th,
  .cross td {
    padding: 3px 6px;
  }
  .cross td {
    text-align: center;
    font-variant-numeric: tabular-nums;
    border: 2px solid var(--panel, var(--bg));
    border-radius: 4px;
    min-width: 3.2em;
    cursor: default;
  }
  .cross td.self {
    color: var(--muted);
  }
  .cross td.outlier {
    font-weight: 600;
    outline: 1px solid var(--text);
    outline-offset: -3px;
  }
  .legend {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 4px;
    margin-top: 6px;
  }
  .swatch {
    display: inline-block;
    width: 12px;
    height: 12px;
    border-radius: 3px;
    margin-left: 6px;
  }
  .mark {
    margin-left: 6px;
    font-weight: 600;
    color: var(--text);
  }
  .outliers {
    margin: 0;
    padding-left: 18px;
  }
  .error {
    color: var(--warn);
  }
  .subtle {
    font-size: 12px;
    padding: 2px 8px;
    color: var(--muted);
  }
</style>
