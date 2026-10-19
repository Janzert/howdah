<script lang="ts">
  // The lobby window's Settings panel: display and sound preferences, and
  // where new boards open. Changes apply at once and are saved as they're
  // made (and reach every window).
  import { MAX_STEP_MS, settings, type Appearance, type Coordinates, type HoverInput } from './settings.svelte';
  import { play } from './sound';
  import { findTheme, themes } from './theme';

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
      hint: 'Hovering a piece shows its legal steps; click an arrow to take one.',
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

<div class="screen">
  <header><h1 id="settings-title">Settings</h1></header>
  <div class="card">
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
        A step after the fourth finishes the turn and starts the other side's. In a game, your move stays a plan until
        you play it; Shift+Enter ends a shorter turn as a plan.
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
              <path
                d="M16 8.5a5 5 0 0 1 0 7M18.5 6a8.5 8.5 0 0 1 0 12"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
              />
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
      <label class="check gap">
        <input type="checkbox" bind:checked={settings.spectatorTicks} />
        Low-time ticks when watching
      </label>
      <p class="hint">
        Tick for both clocks in a game with no human player here: one watched on arimaa.com, or engine against engine.
      </p>
    </section>

    <section>
      <h3>Boards</h3>
      <fieldset class="row">
        <legend>New boards</legend>
        <div class="choices">
          <label>
            <input type="radio" name="boards-in" value={false} bind:group={settings.boardsInWindows} />
            In this window
          </label>
          <label>
            <input type="radio" name="boards-in" value={true} bind:group={settings.boardsInWindows} />
            In their own windows
          </label>
        </div>
      </fieldset>
      <p class="hint">A board can move between the two at any time: Pop out, or Dock in its window.</p>
    </section>

    <section>
      <h3>arimaa.com chat room</h3>
      <label class="check">
        <input type="checkbox" bind:checked={settings.leaveChatForGames} />
        Leave the chat room for rated and event games
      </label>
      <p class="hint">
        When you play a rated game against a person that isn't postal, or any event game, you leave the chat room as it
        starts. You can enter again during the game.
      </p>
    </section>
  </div>
</div>

<style>
  header {
    display: flex;
    align-items: center;
    margin-bottom: 14px;
    min-height: 30px;
  }
  h1 {
    margin: 0;
    font-size: 20px;
  }
  .card {
    max-width: 560px;
    padding-top: 4px;
    padding-bottom: 4px;
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
    padding: 12px 0;
  }
  section + section {
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
</style>
