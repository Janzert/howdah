<script lang="ts">
  import type { ManifestOptionView } from './bindings/ManifestOptionView';
  import { detailText, optionError } from './engineOptions';

  interface Props {
    /** The options to show, from the engine's manifest. */
    described: ManifestOptionView[];
    /** Their values by name; empty leaves the engine's default. */
    values: Record<string, string>;
    /** Starts each field's id, which must be unique on the page. */
    idPrefix: string;
  }
  let { described, values = $bindable(), idPrefix }: Props = $props();

  /** A check option's box: ticked by its value, or else its default. */
  function isTicked(name: string, fallback: string | null): boolean {
    return (values[name] || fallback) === 'true';
  }
</script>

<!-- A label and a field per option, for a two-column grid. -->
{#each described as o (o.name)}
  {@const id = `${idPrefix}-${o.name}`}
  {@const error = optionError(o, values[o.name] ?? '')}
  <label for={id}>{o.name}</label>
  <div class="field">
    {#if o.kind === 'check' && o.default != null}
      <input
        {id}
        type="checkbox"
        checked={isTicked(o.name, o.default)}
        onchange={(e) => (values[o.name] = String(e.currentTarget.checked))}
      />
    {:else if o.kind === 'check'}
      <select {id} bind:value={values[o.name]}>
        <option value={undefined}>Engine default</option>
        <option value="true">On</option>
        <option value="false">Off</option>
      </select>
    {:else if o.kind === 'combo'}
      <select {id} bind:value={values[o.name]}>
        <option value={undefined}>Default{o.default != null ? ` (${o.default})` : ''}</option>
        {#each o.choices as c (c)}<option value={c}>{c}</option>{/each}
      </select>
    {:else if o.kind === 'spin' || o.kind === 'float'}
      <input
        {id}
        class="number"
        class:invalid={error}
        type="number"
        min={o.min}
        max={o.max}
        step={o.kind === 'spin' ? 1 : 'any'}
        value={values[o.name] ?? ''}
        oninput={(e) => (values[o.name] = e.currentTarget.value)}
        placeholder={o.default ?? 'default'}
      />
    {:else}
      <input
        {id}
        class:invalid={error}
        bind:value={values[o.name]}
        placeholder={o.default ?? (o.kind === 'file' ? 'a file' : o.kind === 'path' ? 'a directory' : 'default')}
      />
    {/if}
    <span class="desc">
      {[o.description, detailText(o) && `(${detailText(o)})`].filter(Boolean).join(' ')}
    </span>
  </div>
{/each}

<style>
  input,
  select {
    font: inherit;
    padding: 4px 8px;
    border: 1px solid var(--border);
    border-radius: 5px;
    background: var(--bg);
    color: var(--text);
  }
  .field {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .desc {
    flex: 1;
    min-width: 0;
    font-size: 12px;
    color: var(--muted);
  }
  .number {
    width: 7em;
  }
  .invalid {
    border-color: var(--warn);
  }
</style>
