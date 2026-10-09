<script lang="ts">
  // Asks before a window closes on a game or match that closing would end.
  interface Props {
    /** Why closing needs asking. */
    reason: string;
    /** The button that closes anyway ("Leave game and close"). */
    confirm: string;
    onAnswer: (close: boolean) => void;
  }
  let { reason, confirm, onAnswer }: Props = $props();

  let dialog: HTMLDialogElement;
  let answer = false;

  $effect(() => {
    dialog.showModal();
  });

  function close(yes: boolean) {
    answer = yes;
    dialog.close();
  }
</script>

<dialog bind:this={dialog} onclose={() => onAnswer(answer)} aria-labelledby="close-window-title">
  <h2 id="close-window-title">Close this window?</h2>
  <p>{reason}</p>
  <div class="buttons">
    <span class="spacer"></span>
    <button onclick={() => close(true)}>{confirm}</button>
    <!-- svelte-ignore a11y_autofocus -->
    <button class="primary" onclick={() => close(false)} autofocus>Keep it open</button>
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
    background: rgba(0, 0, 0, 0.25);
  }
  h2 {
    margin: 0 0 8px;
    font-size: 18px;
  }
  p {
    margin: 0;
  }
  .buttons {
    display: flex;
    gap: 6px;
    margin-top: 16px;
    align-items: center;
  }
  .spacer {
    flex: 1;
  }
</style>
