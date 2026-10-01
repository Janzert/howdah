<script lang="ts">
  // Display preferences. Changes apply immediately and are saved as they're made.
  import { settings, type Coordinates } from './settings.svelte';
  import { findTheme, themes } from './theme';

  let { onClose }: { onClose: () => void } = $props();

  let dialog: HTMLDialogElement;

  $effect(() => {
    dialog.showModal();
  });

  const coordinateChoices: { value: Coordinates; label: string }[] = [
    { value: 'none', label: 'None' },
    { value: 'traps', label: 'Traps' },
    { value: 'all', label: 'Files and ranks' },
  ];

  const attribution = $derived(findTheme(settings.theme).attribution);
</script>

<dialog bind:this={dialog} onclose={onClose} aria-labelledby="settings-title">
  <h2 id="settings-title">Settings</h2>

  <section>
    <h3>Board</h3>
    <label class="row">
      <span>Theme</span>
      <select bind:value={settings.theme}>
        {#each themes as t (t.id)}
          <option value={t.id}>{t.name}</option>
        {/each}
      </select>
    </label>
    {#if attribution}<p class="attribution">{attribution}</p>{/if}
    <fieldset class="row">
      <legend>Coordinates</legend>
      <div class="choices">
        {#each coordinateChoices as c (c.value)}
          <label>
            <input type="radio" name="coordinates" value={c.value} bind:group={settings.coordinates} />
            {c.label}
          </label>
        {/each}
      </div>
    </fieldset>
  </section>

  <section>
    <h3>Input</h3>
    <label class="check">
      <input type="checkbox" bind:checked={settings.hoverArrows} />
      Hover arrows: show a piece's legal steps, click an arrow to step
    </label>
  </section>

  <section>
    <h3>Sound</h3>
    <label class="check">
      <input type="checkbox" bind:checked={settings.sound} />
      Play sounds
    </label>
  </section>

  <div class="buttons">
    <button class="primary" onclick={onClose}>Done</button>
  </div>
</dialog>

<style>
  dialog {
    width: min(420px, 90vw);
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--panel);
    color: var(--text);
    padding: 16px;
  }
  dialog::backdrop {
    background: rgba(0, 0, 0, 0.4);
  }
  h2 {
    margin: 0 0 8px;
    font-size: 16px;
  }
  h3 {
    margin: 0 0 6px;
    font-size: 12px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--muted);
  }
  section {
    padding: 10px 0;
    border-top: 1px solid var(--border);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 12px;
    margin: 0 0 8px;
    font-size: 14px;
  }
  .row > span,
  legend {
    width: 96px;
    flex: none;
  }
  fieldset {
    border: none;
    padding: 0;
  }
  legend {
    float: left;
    padding: 0;
  }
  .choices {
    display: flex;
    gap: 12px;
    flex-wrap: wrap;
  }
  .choices label,
  .check {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 14px;
  }
  .attribution {
    margin: -2px 0 10px 108px;
    font-size: 11px;
    color: var(--muted);
  }
  .buttons {
    display: flex;
    justify-content: flex-end;
    margin-top: 8px;
  }
</style>
