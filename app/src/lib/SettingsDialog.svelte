<script lang="ts">
  // Display preferences. Changes apply immediately and are saved as they're made.
  import { MAX_STEP_MS, settings, type Appearance, type Coordinates, type HoverInput } from './settings.svelte';
  import { play } from './sound';
  import { findTheme, themes } from './theme';

  let { onClose }: { onClose: () => void } = $props();

  let dialog: HTMLDialogElement;

  $effect(() => {
    dialog.showModal();
  });

  const appearanceChoices: { value: Appearance; label: string }[] = [
    { value: 'system', label: 'System' },
    { value: 'light', label: 'Light' },
    { value: 'dark', label: 'Dark' },
  ];

  const coordinateChoices: { value: Coordinates; label: string }[] = [
    { value: 'none', label: 'None' },
    { value: 'traps', label: 'Traps' },
    { value: 'all', label: 'Files and ranks' },
  ];

  const hoverChoices: { value: HoverInput; label: string; hint: string }[] = [
    { value: 'off', label: 'Off', hint: 'Drag pieces to move them.' },
    {
      value: 'arrows',
      label: 'Arrows',
      hint: "Hovering a piece shows its legal steps; click an arrow to take one.",
    },
    {
      value: 'step',
      label: 'Step mode',
      hint: 'Shows the step toward the edge of the square the pointer is near; click to take it.',
    },
  ];
  const hoverHint = $derived(hoverChoices.find((c) => c.value === settings.hoverInput)?.hint);

  const attribution = $derived(findTheme(settings.theme).attribution);
</script>

<dialog bind:this={dialog} onclose={onClose} aria-labelledby="settings-title">
  <h2 id="settings-title">Settings</h2>

  <section>
    <h3>Display</h3>
    <fieldset class="row">
      <legend>Appearance</legend>
      <div class="choices">
        {#each appearanceChoices as c (c.value)}
          <label>
            <input type="radio" name="appearance" value={c.value} bind:group={settings.appearance} />
            {c.label}
          </label>
        {/each}
      </div>
    </fieldset>
  </section>

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
    <label class="check">
      <input type="checkbox" bind:checked={settings.humanAtBottom} />
      Put the human player at the bottom
    </label>
    <p class="hint">When a game against an engine starts. Flip still turns the board any time.</p>
    <label class="row slider">
      <span>Animation</span>
      <input type="range" min="0" max={MAX_STEP_MS} step="20" bind:value={settings.stepMs} />
      <span class="value">{settings.stepMs === 0 ? 'instant' : `${settings.stepMs} ms`}</span>
    </label>
    <p class="hint">Time for a piece to slide one step. Moves speed up when several are waiting.</p>
  </section>

  <section>
    <h3>Input</h3>
    <fieldset class="row">
      <legend>Hover</legend>
      <div class="choices">
        {#each hoverChoices as c (c.value)}
          <label>
            <input type="radio" name="hover-input" value={c.value} bind:group={settings.hoverInput} />
            {c.label}
          </label>
        {/each}
      </div>
    </fieldset>
    <p class="hint">{hoverHint} Dragging always works.</p>
    <label class="check gap">
      <input type="checkbox" bind:checked={settings.continueTurns} />
      Keep stepping after a full turn
    </label>
    <p class="hint">
      A step after the fourth finishes the turn and starts the other side's. In a game, your move stays a plan
      until you play it; Shift+Enter ends a shorter turn as a plan.
    </p>
  </section>

  <section>
    <h3>Sound</h3>
    <div class="row">
      <label for="volume">Volume</label>
      <button
        class="mute"
        aria-pressed={!settings.sound}
        aria-label={settings.sound ? 'Mute' : 'Unmute'}
        title={settings.sound ? 'Mute (m)' : 'Unmute (m)'}
        onclick={() => (settings.sound = !settings.sound)}
      >
        <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
          <path d="M4 9h4l5-4v14l-5-4H4z" fill="currentColor" />
          {#if settings.sound}
            <path d="M16 8.5a5 5 0 0 1 0 7M18.5 6a8.5 8.5 0 0 1 0 12" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
          {:else}
            <path d="M16 9l6 6M22 9l-6 6" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
          {/if}
        </svg>
      </button>
      <input
        id="volume"
        type="range"
        min="0"
        max="100"
        step="5"
        class:muted={!settings.sound}
        bind:value={settings.volume}
        oninput={() => (settings.sound = true)}
        onchange={() => play('lastStep')}
      />
      <span class="value">{settings.volume}%</span>
    </div>
    <p class="hint">Moving the slider unmutes.</p>
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
  .row > label,
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
  .gap {
    margin-top: 10px;
  }
  .slider {
    margin-top: 10px;
  }
  input[type='range'] {
    flex: 1;
    min-width: 0;
  }
  input[type='range'].muted {
    opacity: 0.45;
  }
  .row > .mute {
    flex: none;
    display: grid;
    place-items: center;
    width: 30px;
    height: 26px;
    padding: 0;
    margin-left: -6px;
  }
  .mute[aria-pressed='true'] {
    color: var(--warn);
  }
  .row > .value {
    width: 64px;
    flex: none;
    font-size: 12px;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }
  .attribution {
    margin: -2px 0 10px 108px;
    font-size: 11px;
    color: var(--muted);
  }
  .hint {
    margin: -2px 0 0 108px;
    font-size: 12px;
    color: var(--muted);
  }
  .buttons {
    display: flex;
    justify-content: flex-end;
    margin-top: 8px;
  }
</style>
