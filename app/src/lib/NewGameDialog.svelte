<script lang="ts">
  import type { EngineSpec } from './bindings/EngineSpec';
  import type { MatchSpec } from './bindings/MatchSpec';
  import type { PlayerSpec } from './bindings/PlayerSpec';

  interface Props {
    engines: EngineSpec[];
    /** Resolves to an error message, or null on success. */
    onStart: (spec: MatchSpec) => Promise<string | null>;
    onClose: () => void;
    onManageEngines: () => void;
  }
  let { engines, onStart, onClose, onManageEngines }: Props = $props();

  function pref(key: string, fallback: string): string {
    try {
      return localStorage.getItem(key) ?? fallback;
    } catch {
      return fallback;
    }
  }
  function savePref(key: string, value: string) {
    try {
      localStorage.setItem(key, value);
    } catch {
      /* not persisted */
    }
  }

  let gold = $state(pref('newgame.gold', 'human'));
  let silver = $state(pref('newgame.silver', 'human'));
  let timeControl = $state(pref('newgame.tc', ''));
  let error = $state<string | null>(null);
  let dialog: HTMLDialogElement;

  $effect(() => {
    dialog.showModal();
  });

  const presets = ['15s/1m', '30s/2m', '1m/5m', '2m/10m/100/20m', '30s/0/100/0/0t'];

  function player(value: string): PlayerSpec {
    return value === 'human' ? { kind: 'human' } : { kind: 'engine', engineId: value };
  }

  async function start() {
    savePref('newgame.gold', gold);
    savePref('newgame.silver', silver);
    savePref('newgame.tc', timeControl);
    error = await onStart({
      gold: player(gold),
      silver: player(silver),
      timeControl: timeControl.trim() || null,
    });
    if (!error) onClose();
  }
</script>

<dialog bind:this={dialog} onclose={onClose}>
  <h2>New game</h2>
  <div class="grid">
    <label for="ng-gold"><span class="dot gold"></span> Gold</label>
    <select id="ng-gold" bind:value={gold}>
      <option value="human">Human</option>
      {#each engines as e (e.id)}<option value={e.id}>{e.name}</option>{/each}
    </select>
    <label for="ng-silver"><span class="dot silver"></span> Silver</label>
    <select id="ng-silver" bind:value={silver}>
      <option value="human">Human</option>
      {#each engines as e (e.id)}<option value={e.id}>{e.name}</option>{/each}
    </select>
    <label for="ng-tc">Time control</label>
    <div>
      <input id="ng-tc" list="ng-tc-presets" bind:value={timeControl} placeholder="none (e.g. 30s/2m)" />
      <datalist id="ng-tc-presets">
        {#each presets as p (p)}<option value={p}></option>{/each}
      </datalist>
      <p class="hint">move/reserve[/percent/max reserve/game limit/max turn], e.g. <code>30s/2m</code></p>
    </div>
  </div>
  {#if engines.length === 0}
    <p class="hint">No engines configured yet.</p>
  {/if}
  {#if error}<p class="error">{error}</p>{/if}
  <div class="buttons">
    <button onclick={onManageEngines}>Engines…</button>
    <span class="spacer"></span>
    <button onclick={onClose}>Cancel</button>
    <button class="primary" onclick={start}>Start</button>
  </div>
</dialog>

<style>
  dialog {
    width: min(460px, 90vw);
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
    margin: 0 0 12px;
    font-size: 16px;
  }
  .grid {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 8px 12px;
    align-items: start;
  }
  label {
    display: flex;
    align-items: center;
    gap: 6px;
    padding-top: 5px;
  }
  input {
    width: 100%;
    box-sizing: border-box;
    font: inherit;
    padding: 4px 8px;
    border: 1px solid var(--border);
    border-radius: 5px;
    background: var(--bg);
    color: var(--text);
  }
  .dot {
    width: 11px;
    height: 11px;
    border-radius: 50%;
    border: 1px solid rgba(0, 0, 0, 0.4);
  }
  .dot.gold {
    background: #e3b23c;
  }
  .dot.silver {
    background: #c9ced6;
  }
  .hint {
    margin: 4px 0 0;
    font-size: 12px;
    color: var(--muted);
  }
  .error {
    color: var(--warn);
    font-size: 13px;
  }
  .buttons {
    display: flex;
    gap: 6px;
    margin-top: 14px;
  }
  .spacer {
    flex: 1;
  }
</style>
