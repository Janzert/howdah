<script lang="ts" module>
  /** Common arimaa.com time controls (names as on the site). */
  export const PRESETS: { name: string; value: string }[] = [
    { name: 'Lightning 8 sec/move', value: '8s/2m16s/100/0/32m/2m16s' },
    { name: 'Blitz 15 sec/move', value: '15s/2m30s/100/0/1h/2m30s' },
    { name: 'Fast 30 sec/move', value: '30s/3m/100/0/2h/3m' },
    { name: 'Regular 1 min/move', value: '1m/4m/100/0/4h/4m' },
    { name: 'Match 1.5 min/move', value: '1m30s/5m/100/0/6h/5m' },
    { name: 'Slow 4 min/move', value: '4m/10m/100/0/16h/10m' },
  ];
</script>

<script lang="ts">
  interface Props {
    /** The time control text; empty means no clock. */
    value: string;
    id?: string;
  }
  let { value = $bindable(), id }: Props = $props();

  // Which preset the text matches, if any. Picking one fills in the text,
  // which stays editable.
  const selected = $derived.by(() => {
    const v = value.trim();
    if (v === '') return 'none';
    return PRESETS.find((p) => p.value === v)?.value ?? 'custom';
  });

  function pick(e: Event) {
    const choice = (e.currentTarget as HTMLSelectElement).value;
    if (choice === 'none') value = '';
    else if (choice !== 'custom') value = choice;
  }
</script>

<div class="tc">
  <select value={selected} onchange={pick} aria-label="Time control preset">
    <option value="none">No clock</option>
    {#each PRESETS as p (p.value)}<option value={p.value}>{p.name}</option>{/each}
    <option value="custom" disabled={selected !== 'custom'}>Custom</option>
  </select>
  <input {id} bind:value placeholder="none (e.g. 30s/2m)" spellcheck="false" />
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
