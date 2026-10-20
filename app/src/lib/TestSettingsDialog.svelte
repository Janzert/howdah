<script lang="ts">
  // A running test's settings that can change along the way: its pair
  // limit, its undecided cutoff (an SPRT test's), how many of its games run
  // at once, and its priority. Saving sends only what changed.
  import { untrack } from 'svelte';
  import { api, errorMessage } from './api';
  import type { TestView } from './bindings/TestView';
  import { cutoffPercent, pairCount, percentCutoff } from './tests';

  interface Props {
    test: TestView;
    onSaved: (t: TestView) => void;
    onClose: () => void;
  }
  let { test, onSaved, onClose }: Props = $props();

  // The fields start from the test as it was when the dialog opened.
  const start = untrack(() => test);
  let maxPairs = $state<number | null>(start.maxPairs);
  /** The undecided cutoff in percent (empty: off). */
  let undecided = $state<number | null>(cutoffPercent(start));
  let concurrency = $state(start.concurrency);
  let priority = $state(start.priority);
  let error = $state<string | null>(null);
  let busy = $state(false);
  let dialog: HTMLDialogElement;

  $effect(() => {
    dialog.showModal();
  });

  const pairs = $derived(pairCount(maxPairs));
  const canSave = $derived(!!test.sprt || (pairs != null && pairs >= 1));

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    error = null;
    const id = test.id;
    let t = test;
    try {
      if (pairs !== t.maxPairs) t = await api.tourneySetMaxPairs(id, pairs);
      const cutoff = pairs != null ? percentCutoff(undecided) : null;
      if (t.sprt && (cutoff == null ? null : Number(undecided)) !== cutoffPercent(t)) {
        t = await api.tourneySetUndecidedBelow(id, cutoff);
      }
      if (concurrency !== t.concurrency) t = await api.tourneySetConcurrency(id, concurrency);
      if (priority !== t.priority) t = await api.tourneySetPriority(id, priority);
      onSaved(t);
      dialog.close();
    } catch (err) {
      // What got through before the error still shows in the panel.
      if (t !== test) onSaved(t);
      error = errorMessage(err);
    }
    busy = false;
  }
</script>

<dialog bind:this={dialog} onclose={onClose} aria-labelledby="test-settings-title">
  <h2 id="test-settings-title">Test settings</h2>
  <p class="note">{test.name}</p>
  <form onsubmit={submit}>
    <label for="ts-pairs">Max pairs</label>
    <div class="field">
      <input
        id="ts-pairs"
        type="number"
        min="1"
        bind:value={maxPairs}
        placeholder={test.sprt ? 'none' : 'needed'}
        title="End the test after this many pairs of games"
      />
      <span class="hint"
        >{test.sprt
          ? 'End after this many pairs if the SPRT hasn’t decided (empty: no limit).'
          : 'End after this many pairs.'}
        Pairs played so far: {test.pairCounts[0] + test.pairCounts[1] + test.pairCounts[2]}.</span
      >
    </div>
    {#if test.sprt}
      <label
        for="ts-undecided"
        title="End the test undecided once the SPRT's chance of deciding within the max pairs falls below this percentage. At most about that share of tests ended this way would have gone on to pass or fail. Empty: play on to the limit."
        >Undecided under</label
      >
      <div class="field">
        <div class="inline">
          <input
            id="ts-undecided"
            type="number"
            min="0"
            max="99"
            step="any"
            bind:value={undecided}
            placeholder="off"
            disabled={pairs == null}
          />
          <span>% chance of deciding within the max pairs</span>
        </div>
        <span class="hint"
          >{pairs == null
            ? 'Needs a max pairs.'
            : 'Ends the test early once it is unlikely to pass or fail before the limit (empty: play on to the limit).'}</span
        >
      </div>
    {/if}
    <label for="ts-conc">Games at once</label>
    <div class="field">
      <input id="ts-conc" type="number" min="1" bind:value={concurrency} />
      <span class="hint"
        >How many of this test's games may run at the same time. The daemon's own limit still applies.</span
      >
    </div>
    <label for="ts-priority">Priority</label>
    <div class="field">
      <input id="ts-priority" type="number" bind:value={priority} />
      <span class="hint">When tests compete for free game slots, higher priority goes first.</span>
    </div>
    {#if error}<p class="error wide" role="alert">{error}</p>{/if}
    <div class="buttons wide">
      <span class="spacer"></span>
      <button type="button" onclick={() => dialog.close()}>Cancel</button>
      <button type="submit" class="primary" disabled={busy || !canSave}>Save</button>
    </div>
  </form>
</dialog>

<style>
  dialog {
    width: min(520px, 92vw);
  }
  h2 {
    margin: 0 0 4px;
    font-size: 16px;
  }
  .note {
    margin: 0 0 10px;
    font-size: 12px;
    color: var(--muted);
  }
  form {
    display: grid;
    grid-template-columns: max-content 1fr;
    align-items: baseline;
    gap: 10px 10px;
    font-size: 13px;
  }
  .field {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
  }
  .inline {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
  }
  .hint {
    font-size: 12px;
    color: var(--muted);
  }
  input[type='number'] {
    width: 6em;
  }
  .wide {
    grid-column: 1 / -1;
  }
  .error {
    color: var(--warn);
    margin: 0;
  }
  .buttons {
    display: flex;
    gap: 6px;
    margin-top: 6px;
  }
  .spacer {
    flex: 1;
  }
</style>
