<script lang="ts">
  // A new engine test: two engines from the engine list (engine 1 is the
  // one being tested), a time control, and when to stop: when the SPRT
  // decides, after a number of pairs, or whichever comes first. Or a pool:
  // two or more engines rated against each other until stopped (or a pair
  // limit), taking more engines as it goes. The daemon copies the engines,
  // so rebuilding one doesn't change the test.
  import { api, errorMessage } from './api';
  import type { EngineSpec } from './bindings/EngineSpec';
  import type { TestView } from './bindings/TestView';
  import { pairCount, percentCutoff, TEST_TIME_CONTROLS, UNDECIDED_PERCENT } from './tests';

  interface Props {
    engines: EngineSpec[];
    onCreated: (t: TestView) => void;
    onClose: () => void;
  }
  let { engines, onCreated, onClose }: Props = $props();

  // The two engines, by id; until picked, the first two in the list.
  let picked = $state<[string | null, string | null]>([null, null]);
  const first = $derived(picked[0] ?? engines[0]?.id ?? '');
  const second = $derived(picked[1] ?? engines[1]?.id ?? engines[0]?.id ?? '');
  /** A pool instead of a two-engine test. */
  let isPool = $state(false);
  /** A pool's engines, by id. */
  let members = $state<string[]>([]);
  let name = $state('');
  let timeControl = $state(TEST_TIME_CONTROLS[1][0]);
  let useSprt = $state(true);
  let elo0 = $state(0);
  let elo1 = $state(10);
  let maxPairs = $state<number | null>(null);
  /** The undecided cutoff in percent (empty: off). */
  let undecided = $state<number | null>(UNDECIDED_PERCENT);
  let concurrency = $state(1);
  let priority = $state(0);
  let error = $state<string | null>(null);
  let busy = $state(false);
  let dialog: HTMLDialogElement;

  $effect(() => {
    dialog.showModal();
  });

  const nameOf = (id: string) => engines.find((e) => e.id === id)?.name ?? id;
  const placeholder = $derived(
    isPool
      ? members.length > 0
        ? `Pool of ${members.map(nameOf).join(', ')}`
        : 'Pool'
      : `${nameOf(first)} vs ${nameOf(second)}`,
  );
  const pairs = $derived(pairCount(maxPairs));
  const canStart = $derived(
    isPool ? members.length >= 2 : !!first && !!second && (useSprt || (pairs != null && pairs >= 1)),
  );

  function toggleMember(id: string, on: boolean) {
    members = on ? [...members.filter((m) => m !== id), id] : members.filter((m) => m !== id);
  }

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    error = null;
    try {
      const t = await api.tourneySubmit({
        name: name.trim() || placeholder,
        engines: isPool ? engines.filter((e) => members.includes(e.id)).map((e) => e.id) : [first, second],
        pool: isPool,
        timeControl: timeControl.trim() || null,
        maxPairs: pairs,
        sprt:
          useSprt && !isPool ? { elo0, elo1, undecidedBelow: pairs != null ? percentCutoff(undecided) : null } : null,
        concurrency,
        priority,
      });
      onCreated(t);
    } catch (err) {
      error = errorMessage(err);
    }
    busy = false;
  }
</script>

<dialog bind:this={dialog} onclose={onClose} aria-labelledby="new-test-title">
  <h2 id="new-test-title">New engine test</h2>
  <div class="kind" role="radiogroup" aria-label="Kind of test">
    <label class="check"><input type="radio" bind:group={isPool} value={false} /> Two engines</label>
    <label class="check"><input type="radio" bind:group={isPool} value={true} /> Pool</label>
  </div>
  <p class="note">
    {#if isPool}
      Rates the engines against each other from all their games, for as long as it runs, picking each next pairing by
      what it would tell. Engines can be added or retired while it runs.
    {:else}
      Plays the two engines in pairs of games, each taking gold once, and says whether the first is stronger.
    {/if}
  </p>
  {#if engines.length === 0}
    <p class="error">Add engines first (Engines).</p>
  {/if}
  <form onsubmit={submit}>
    {#if isPool}
      <span class="label">Engines</span>
      <div class="members" role="group" aria-label="The pool's engines">
        {#each engines as e (e.id)}
          <label class="check"
            ><input
              type="checkbox"
              checked={members.includes(e.id)}
              onchange={(ev) => toggleMember(e.id, ev.currentTarget.checked)}
            />
            {e.name}</label
          >
        {/each}
      </div>
    {:else}
      <label for="nt-first">Engine to test</label>
      <select id="nt-first" value={first} onchange={(e) => (picked[0] = e.currentTarget.value)}>
        {#each engines as e (e.id)}<option value={e.id}>{e.name}</option>{/each}
      </select>
      <label for="nt-second">Against</label>
      <select id="nt-second" value={second} onchange={(e) => (picked[1] = e.currentTarget.value)}>
        {#each engines as e (e.id)}<option value={e.id}>{e.name}</option>{/each}
      </select>
    {/if}
    <label for="nt-name">Name</label>
    <input id="nt-name" bind:value={name} {placeholder} autocomplete="off" />
    <label for="nt-tc">Time control</label>
    <input
      id="nt-tc"
      bind:value={timeControl}
      list="nt-tcs"
      autocomplete="off"
      title="Per move / reserve / percent added / reserve limit / game or turn limit (100t: 100 turns)"
    />
    <datalist id="nt-tcs">
      {#each TEST_TIME_CONTROLS as [tc, label] (tc)}<option value={tc} {label}></option>{/each}
    </datalist>
    {#if !isPool}
      <span class="label">Stop</span>
      <div class="stop">
        <label class="check"><input type="checkbox" bind:checked={useSprt} /> when the SPRT decides between Elo</label>
        <input type="number" bind:value={elo0} aria-label="Elo 0 (no better than)" disabled={!useSprt} />
        <span>and</span>
        <input type="number" bind:value={elo1} aria-label="Elo 1 (at least this much better)" disabled={!useSprt} />
      </div>
    {/if}
    <label for="nt-pairs">Max pairs</label>
    <input
      id="nt-pairs"
      type="number"
      min="1"
      bind:value={maxPairs}
      placeholder={useSprt || isPool ? 'none' : 'needed'}
    />
    {#if !isPool}
      <label
        for="nt-undecided"
        title="End the test undecided once the SPRT's chance of deciding within the max pairs falls below this percentage. At most about that share of tests ended this way would have gone on to pass or fail. Empty: play on to the limit."
        >Undecided under</label
      >
      <div class="stop">
        <input
          id="nt-undecided"
          type="number"
          min="0"
          max="99"
          step="any"
          bind:value={undecided}
          placeholder="off"
          disabled={!useSprt || pairs == null}
        />
        <span>% chance of deciding within the max pairs</span>
      </div>
    {/if}
    <label for="nt-conc">Games at once</label>
    <input id="nt-conc" type="number" min="1" bind:value={concurrency} />
    <label for="nt-priority">Priority</label>
    <input id="nt-priority" type="number" bind:value={priority} title="Higher priority tests get free slots first" />
    {#if error}<p class="error wide" role="alert">{error}</p>{/if}
    <div class="buttons wide">
      <span class="spacer"></span>
      <button type="button" onclick={() => dialog.close()}>Cancel</button>
      <button type="submit" class="primary" disabled={busy || !canStart}>Start</button>
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
    align-items: center;
    gap: 8px 10px;
    font-size: 13px;
  }
  .kind {
    display: flex;
    gap: 14px;
    margin-bottom: 6px;
    font-size: 13px;
  }
  .members {
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: 12em;
    overflow-y: auto;
  }
  .stop {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
  }
  .stop input[type='number'] {
    width: 4.5em;
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
