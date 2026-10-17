<script lang="ts" module>
  /** Time controls offered; any other can be typed. The postal ones are
   * the gameroom's own ("No time limit" is its `0/0/0/0/0`). */
  export const GAMEROOM_TIME_CONTROLS: { value: string; name: string }[] = [
    { value: '15s/3m/100/0/15m', name: '15 s a move' },
    { value: '1m/2m/100/0/10m', name: '1 min a move' },
    { value: '2m/5m/100/0/30m', name: '2 min a move' },
    { value: '1d/60d/100/0/300d/21d', name: 'Postal: a day a move, the most popular' },
    { value: '1d/14d/100/14d/0', name: 'Postal: a day a move, 14 day reserve' },
    { value: '0/0/0/0/0', name: 'Postal: no time limit' },
  ];
</script>

<script lang="ts">
  // A new arimaa.com game: open to anyone (it waits in the gameroom's open
  // games), or an invitation to one player. The choices are kept in
  // localStorage, the opponent and message aren't.
  import { untrack } from 'svelte';
  import type { Color } from '../bindings/Color';
  import { gameroom, type NewGameSpec, pref, savePref } from './store.svelte';

  let { initial, onClose }: { initial: Partial<NewGameSpec>; onClose: () => void } = $props();

  const start = untrack(() => initial);
  // Only errors from this dialog's own request.
  gameroom.error = null;
  let side = $state<Color | 'random'>(start.side ?? ((pref('gameroom.newSide') as Color | 'random') || 'random'));
  let timeControl = $state(start.timeControl ?? (pref('gameroom.newTc') || '2m/5m/100/0/30m'));
  let rated = $state(start.rated ?? pref('gameroom.newRated') === '1');
  let opponent = $state(start.opponent ?? '');
  let message = $state(start.message ?? '');

  const preset = $derived(GAMEROOM_TIME_CONTROLS.find((p) => p.value === timeControl.trim())?.value ?? 'custom');
  const inviting = $derived(opponent.trim() !== '');
  /** Players to suggest for the opponent: those Invite is offered for. */
  const suggestions = $derived([...gameroom.invitable].sort());

  let dialog: HTMLDialogElement;
  $effect(() => {
    dialog.showModal();
  });

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    savePref('gameroom.newSide', side);
    savePref('gameroom.newTc', timeControl.trim());
    savePref('gameroom.newRated', rated ? '1' : '0');
    if (await gameroom.createGame({ side, timeControl, rated, opponent, message })) onClose();
  }
</script>

<dialog bind:this={dialog} onclose={onClose} aria-labelledby="gr-new-title">
  <form onsubmit={submit}>
    <h2 id="gr-new-title">New arimaa.com game</h2>
    <div class="grid">
      <label for="gr-new-opponent">Opponent</label>
      <div>
        <!-- svelte-ignore a11y_autofocus -->
        <input
          id="gr-new-opponent"
          bind:value={opponent}
          list="gr-new-players"
          placeholder="Anyone"
          autocomplete="off"
          autofocus={!inviting}
        />
        <datalist id="gr-new-players">
          {#each suggestions as name (name)}<option value={name}></option>{/each}
        </datalist>
      </div>
      {#if inviting}
        <label for="gr-new-message">Message</label>
        <!-- svelte-ignore a11y_autofocus -->
        <input id="gr-new-message" bind:value={message} placeholder="Optional" autocomplete="off" autofocus />
      {/if}
      <span class="label" id="gr-new-side-label">You play</span>
      <div class="segmented" role="radiogroup" aria-labelledby="gr-new-side-label">
        {#each [['random', 'Random'], ['gold', 'Gold'], ['silver', 'Silver']] as [value, label] (value)}
          <label class:on={side === value}>
            <input type="radio" name="gr-new-side" {value} bind:group={side} />
            {#if value !== 'random'}<span class="dot {value}"></span>{/if}{label}
          </label>
        {/each}
      </div>
      <label for="gr-new-tc">Time control</label>
      <div class="tc">
        <select
          value={preset}
          aria-label="Time control preset"
          onchange={(e) => {
            const v = e.currentTarget.value;
            if (v !== 'custom') timeControl = v;
          }}
        >
          {#each GAMEROOM_TIME_CONTROLS as p (p.value)}<option value={p.value}>{p.name}</option>{/each}
          <option value="custom" disabled={preset !== 'custom'}>Custom</option>
        </select>
        <input
          id="gr-new-tc"
          bind:value={timeControl}
          autocomplete="off"
          spellcheck="false"
          title="Per move / reserve / percent added / reserve limit / game limit"
        />
      </div>
      <span></span>
      <label class="check"><input type="checkbox" bind:checked={rated} /> Rated</label>
    </div>
    <p class="hint">
      {#if inviting}
        Invites {opponent.trim()}; if they accept, the game opens in a new window.
      {:else}
        The game waits in the gameroom's open games until someone sits; it opens in a new window now, and your
        first move goes once they do.
      {/if}
    </p>
    {#if gameroom.error}<p class="error" role="alert">{gameroom.error}</p>{/if}
    <div class="buttons">
      <span class="spacer"></span>
      <button type="button" onclick={() => dialog.close()}>Cancel</button>
      <button class="primary" type="submit" disabled={gameroom.busy || !timeControl.trim()}>
        {inviting ? 'Send invitation' : 'Create game'}
      </button>
    </div>
  </form>
</dialog>

<style>
  dialog {
    width: min(460px, 90vw);
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--panel);
    color: var(--text);
    padding: 18px 20px;
  }
  dialog::backdrop {
    background: rgba(0, 0, 0, 0.4);
  }
  h2 {
    margin: 0 0 14px;
    font-size: 16px;
  }
  .grid {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 10px 14px;
    align-items: center;
    font-size: 13px;
  }
  .grid input:not([type='checkbox'], [type='radio']) {
    width: 100%;
    box-sizing: border-box;
  }
  .tc {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .tc input {
    font-family: ui-monospace, 'DejaVu Sans Mono', monospace;
  }
  .segmented {
    display: inline-flex;
    justify-self: start;
    border: 1px solid var(--border);
    border-radius: 6px;
    overflow: hidden;
  }
  .segmented label {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 4px 12px;
    cursor: pointer;
  }
  .segmented label + label {
    border-left: 1px solid var(--border);
  }
  .segmented label.on {
    background: var(--accent);
    color: var(--accent-text);
  }
  .segmented input {
    position: absolute;
    opacity: 0;
    pointer-events: none;
  }
  .segmented label:focus-within {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }
  .dot {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    border: 1px solid rgba(0, 0, 0, 0.4);
  }
  .dot.gold {
    background: #e3b23c;
  }
  .dot.silver {
    background: #c9ced6;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .hint {
    margin: 14px 0 0;
    font-size: 12px;
    color: var(--muted);
  }
  .error {
    color: var(--warn);
    font-size: 13px;
  }
  .buttons {
    display: flex;
    gap: 8px;
    margin-top: 16px;
  }
  .spacer {
    flex: 1;
  }
</style>
