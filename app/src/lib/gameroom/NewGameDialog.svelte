<script lang="ts">
  // A new arimaa.com game: open to anyone (it waits in the gameroom's open
  // games), or an invitation to one player. The choices are kept in
  // localStorage, the opponent and message aren't.
  import { untrack } from 'svelte';
  import type { Color } from '../bindings/Color';
  import SidePicker from '../SidePicker.svelte';
  import TimeControlInput, { NO_TIME_LIMIT } from '../TimeControlInput.svelte';
  import { gameroom, type NewGameSpec, pref, savePref } from './store.svelte';

  let { initial, onClose }: { initial: Partial<NewGameSpec>; onClose: () => void } = $props();

  const start = untrack(() => initial);
  // Only errors from this dialog's own request.
  gameroom.error = null;
  let side = $state<Color | 'random'>(start.side ?? ((pref('gameroom.newSide') as Color | 'random') || 'random'));
  let timeControl = $state(start.timeControl ?? (pref('gameroom.newTc') || '1m/4m/100/0/4h/4m'));
  let rated = $state(start.rated ?? pref('gameroom.newRated') === '1');
  let opponent = $state(start.opponent ?? '');
  let message = $state(start.message ?? '');

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
      <SidePicker bind:value={side} name="gr-new-side" labelledby="gr-new-side-label" />
      <label for="gr-new-tc">Time control</label>
      <TimeControlInput id="gr-new-tc" bind:value={timeControl} none={NO_TIME_LIMIT} />
      <span></span>
      <label class="check"><input type="checkbox" bind:checked={rated} /> Rated</label>
    </div>
    <p class="hint">
      {#if inviting}
        Invites {opponent.trim()}; if they accept, the game opens on a new board.
      {:else}
        The game waits in the gameroom's open games until someone sits; it opens on a new board now, and your first move
        goes once they do.
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
