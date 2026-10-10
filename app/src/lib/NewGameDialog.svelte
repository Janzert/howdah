<script lang="ts">
  import type { Color } from './bindings/Color';
  import type { EngineOption } from './bindings/EngineOption';
  import type { EngineSpec } from './bindings/EngineSpec';
  import type { MatchSpec } from './bindings/MatchSpec';
  import type { PlayerSpec } from './bindings/PlayerSpec';
  import type { PositionSpec } from './bindings/PositionSpec';
  import GameOptionsDialog from './GameOptionsDialog.svelte';
  import TimeControlInput from './TimeControlInput.svelte';

  interface Props {
    engines: EngineSpec[];
    /** A set position to start from (the position editor's Play). */
    start?: PositionSpec | null;
    /** Resolves to an error message, or null on success. */
    onStart: (spec: MatchSpec) => Promise<string | null>;
    onClose: () => void;
    onManageEngines: () => void;
  }
  let { engines, start: from = null, onStart, onClose, onManageEngines }: Props = $props();

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
  let separate = $state(pref('newgame.separate', '0') === '1');
  let goldTc = $state(pref('newgame.tc.gold', ''));
  let silverTc = $state(pref('newgame.tc.silver', ''));
  let takebacks = $state(pref('newgame.takebacks', '0') === '1');
  /** Each side's engine options for this game, cleared when its engine changes. */
  let gameOptions = $state<Record<Color, EngineOption[]>>({ gold: [], silver: [] });
  let optionsFor = $state<Color | null>(null);
  let error = $state<string | null>(null);
  let dialog: HTMLDialogElement;

  $effect(() => {
    dialog.showModal();
  });

  function toggleSeparate() {
    // Start both sides from the shared control, or go back to gold's.
    if (separate) {
      goldTc = timeControl;
      silverTc = timeControl;
    } else {
      timeControl = goldTc;
    }
  }

  function player(value: string, options: EngineOption[]): PlayerSpec {
    return value === 'human' ? { kind: 'human' } : { kind: 'engine', engineId: value, options };
  }

  const engineFor = (side: Color) => engines.find((e) => e.id === (side === 'gold' ? gold : silver));

  async function start() {
    savePref('newgame.gold', gold);
    savePref('newgame.silver', silver);
    savePref('newgame.tc', timeControl);
    savePref('newgame.separate', separate ? '1' : '0');
    savePref('newgame.tc.gold', goldTc);
    savePref('newgame.tc.silver', silverTc);
    savePref('newgame.takebacks', takebacks ? '1' : '0');
    const [g, s] = separate ? [goldTc, silverTc] : [timeControl, timeControl];
    error = await onStart({
      gold: player(gold, gameOptions.gold),
      silver: player(silver, gameOptions.silver),
      goldTimeControl: g.trim() || null,
      silverTimeControl: s.trim() || null,
      takebacks,
      ...(from ? { start: from } : {}),
    });
    if (!error) onClose();
  }
</script>

{#snippet side(color: Color, id: string)}
  {@const n = gameOptions[color].length}
  <div class="player">
    {#if color === 'gold'}
      <select {id} bind:value={gold} onchange={() => (gameOptions.gold = [])}>
        <option value="human">Human</option>
        {#each engines as e (e.id)}<option value={e.id}>{e.name}</option>{/each}
      </select>
    {:else}
      <select {id} bind:value={silver} onchange={() => (gameOptions.silver = [])}>
        <option value="human">Human</option>
        {#each engines as e (e.id)}<option value={e.id}>{e.name}</option>{/each}
      </select>
    {/if}
    <button
      disabled={!engineFor(color)}
      title="This engine's options for this game"
      onclick={() => (optionsFor = color)}>Options{n ? ` (${n})` : ''}…</button
    >
  </div>
{/snippet}

<dialog bind:this={dialog} onclose={onClose} aria-labelledby="new-game-title">
  <h2 id="new-game-title">New game</h2>
  {#if from}
    <p class="from">
      From the edited position: {from.sideToMove === 'gold' ? 'Gold' : 'Silver'} to move, move {from.moveNumber}
    </p>
  {/if}
  <div class="grid">
    <label for="ng-gold"><span class="dot gold"></span> Gold</label>
    {@render side('gold', 'ng-gold')}
    <label for="ng-silver"><span class="dot silver"></span> Silver</label>
    {@render side('silver', 'ng-silver')}
    {#if separate}
      <label for="ng-tc-gold"><span class="dot gold"></span> Gold clock</label>
      <TimeControlInput id="ng-tc-gold" bind:value={goldTc} />
      <label for="ng-tc-silver"><span class="dot silver"></span> Silver clock</label>
      <TimeControlInput id="ng-tc-silver" bind:value={silverTc} />
    {:else}
      <label for="ng-tc">Time control</label>
      <TimeControlInput id="ng-tc" bind:value={timeControl} />
    {/if}
    <span></span>
    <div>
      <label class="check">
        <input type="checkbox" bind:checked={separate} onchange={toggleSeparate} />
        Separate time control for each player
      </label>
      <p class="hint">move/reserve[/percent/max reserve/game limit/max turn], e.g. <code>30s/2m</code></p>
    </div>
    <span></span>
    <div>
      <label class="check">
        <input type="checkbox" bind:checked={takebacks} />
        Allow takebacks
      </label>
      <p class="hint">Undo played moves (Backspace or Take back), back to a human's move; the clocks go back too.</p>
    </div>
  </div>
  {#if engines.length === 0}
    <p class="hint">No engines configured yet.</p>
  {/if}
  {#if error}<p class="error">{error}</p>{/if}
  <div class="buttons">
    <button onclick={onManageEngines}>Engines</button>
    <span class="spacer"></span>
    <button onclick={onClose}>Cancel</button>
    <button class="primary" onclick={start}>Start</button>
  </div>
</dialog>
{#if optionsFor}
  {@const color = optionsFor}
  {@const engine = engineFor(color)}
  {#if engine}
    <GameOptionsDialog
      {engine}
      current={gameOptions[color]}
      running={false}
      onApply={async (options) => {
        gameOptions[color] = options;
        return null;
      }}
      onClose={() => (optionsFor = null)}
    />
  {/if}
{/if}

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
  .from {
    margin: -6px 0 12px;
    font-size: 13px;
    color: var(--muted);
  }
  .player {
    display: flex;
    gap: 6px;
  }
  .player select {
    flex: 1;
    min-width: 0;
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
  .check {
    padding-top: 0;
    font-size: 13px;
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
