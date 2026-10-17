<script lang="ts" module>
  /** arimaa.com's preset time controls (as its Create Game form offers
   * them, names included), for local games and arimaa.com's New game
   * alike. */
  export const PRESETS: { name: string; value: string }[] = [
    { name: 'Lightning 8 sec/move', value: '8s/2m16s/100/0/32m/2m16s' },
    { name: 'Blitz 15 sec/move', value: '15s/2m30s/100/0/1h/2m30s' },
    { name: 'Fast 30 sec/move', value: '30s/3m/100/0/2h/3m' },
    { name: 'Quick 45 sec/move', value: '45s/3m30s/100/0/3h/3m30s' },
    { name: 'Regular 1 min/move', value: '1m/4m/100/0/4h/4m' },
    { name: 'Match 1.5 min/move', value: '1m30s/5m/100/0/6h/5m' },
    { name: 'Gradual 2 min/move', value: '2m/6m/100/0/8h/6m' },
    { name: 'Relaxed 3 min/move', value: '3m/8m/100/0/12h/8m' },
    { name: 'Slow 4 min/move', value: '4m/10m/100/0/16h/10m' },
    { name: 'Postal: Most Popular 1d/60d/100/0/300d/21d', value: '1d/60d/100/0/300d/21d' },
    { name: 'Postal: Move in 1 day with 14 day reserve', value: '1d/14d/100/14d/0' },
  ];

  /** The first choice: no clock in a local game. */
  export const NO_CLOCK = { name: 'No clock', value: '' };
  /** arimaa.com's "No time limit", a postal game without clocks. */
  export const NO_TIME_LIMIT = { name: 'Postal: No time limit', value: '0/0/0/0/0' };
</script>

<script lang="ts">
  interface Props {
    /** The time control text; `none.value` means no clock. */
    value: string;
    id?: string;
    /** The first choice (`NO_CLOCK`, or arimaa.com's `NO_TIME_LIMIT`). */
    none?: { name: string; value: string };
  }
  let { value = $bindable(), id, none = NO_CLOCK }: Props = $props();

  // Which preset the text matches, if any. Picking one fills in the text,
  // which stays editable.
  const selected = $derived.by(() => {
    const v = value.trim();
    if (v === none.value) return 'none';
    return PRESETS.find((p) => p.value === v)?.value ?? 'custom';
  });

  function pick(e: Event) {
    const choice = (e.currentTarget as HTMLSelectElement).value;
    if (choice === 'none') value = none.value;
    else if (choice !== 'custom') value = choice;
  }
</script>

<div class="tc">
  <select value={selected} onchange={pick} aria-label="Time control preset">
    <option value="none">{none.name}</option>
    {#each PRESETS as p (p.value)}<option value={p.value}>{p.name}</option>{/each}
    <option value="custom" disabled={selected !== 'custom'}>Custom</option>
  </select>
  <input
    {id}
    bind:value
    placeholder={none.value === '' ? 'none (e.g. 30s/2m)' : 'e.g. 1m/4m/100/0/4h'}
    spellcheck="false"
    title="Per move / reserve / percent added / reserve limit / game limit / turn limit"
  />
</div>

<style>
  .tc {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  input {
    width: 100%;
    box-sizing: border-box;
    font: inherit;
    font-family: ui-monospace, 'DejaVu Sans Mono', monospace;
    font-size: 13px;
    padding: 4px 8px;
    border: 1px solid var(--border);
    border-radius: 5px;
    background: var(--bg);
    color: var(--text);
  }
</style>
