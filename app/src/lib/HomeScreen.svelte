<script lang="ts">
  // The lobby window's Home panel: ways to start (new game, analysis
  // board, record, position editor, puzzles), what waits on the user, and
  // the open boards as cards.
  import type { Theme } from './theme';
  import Icon, { type IconName } from './Icon.svelte';
  import MiniBoard from './board/MiniBoard.svelte';
  import type { ListedWindow } from './lobbyWindows';

  interface Props {
    windows: ListedWindow[];
    theme: Theme;
    /** The boards docked as panels here (the others have windows). */
    docked: number[];
    /** Invitations to the user on arimaa.com, and their postal games
     * waiting on their move without a window. */
    invitations: number;
    postalTurns: number;
    onNewGame: () => void;
    onAnalysis: () => void;
    onRecord: () => void;
    onEditor: () => void;
    onPuzzles: () => void;
    onWindow: (w: ListedWindow) => void;
    onGameroom: () => void;
  }
  let { windows, theme, docked, invitations, postalTurns, onNewGame, onAnalysis, onRecord, onEditor, onPuzzles, onWindow, onGameroom }:
    Props = $props();

  const actions = $derived<{ label: string; about: string; icon: IconName; run: () => void; primary?: boolean }[]>([
    { label: 'New game', about: 'Against an engine, a friend, or engines against each other', icon: 'plus', run: onNewGame, primary: true },
    { label: 'Analysis board', about: 'An empty board with an engine analysing', icon: 'board', run: onAnalysis },
    { label: 'Open record', about: 'A game record from a file or pasted text', icon: 'record', run: onRecord },
    { label: 'Position editor', about: 'Set up a position to play or analyse from', icon: 'edit', run: onEditor },
    { label: 'Puzzles', about: "arimaa.com's puzzles, to solve", icon: 'bulb', run: onPuzzles },
  ]);
</script>

<div class="screen">
  <header><h1>Home</h1></header>

  {#if invitations > 0 || postalTurns > 0}
    <div class="attention" role="status">
      <Icon name="globe" />
      <span>
        {#if invitations > 0}{invitations === 1 ? 'An invitation' : `${invitations} invitations`} to a game{/if}
        {#if invitations > 0 && postalTurns > 0}, and{/if}
        {#if postalTurns > 0}{postalTurns === 1 ? 'a postal game' : `${postalTurns} postal games`} on your move{/if}
        on arimaa.com
      </span>
      <span class="spacer"></span>
      <button onclick={onGameroom}>Show</button>
    </div>
  {/if}

  <section aria-labelledby="home-title">
    <h2 id="home-title">Play and analyse</h2>
    <div class="actions">
      {#each actions as a (a.label)}
        <button class="action" class:primary={a.primary} onclick={a.run} aria-label={a.label}>
          <span class="action-icon"><Icon name={a.icon} size={22} /></span>
          <span class="action-text">
            <span class="action-label">{a.label}</span>
            <span class="action-about">{a.about}</span>
          </span>
        </button>
      {/each}
    </div>
  </section>

  <section aria-labelledby="windows-title">
    <h2 id="windows-title">Open boards</h2>
    {#if windows.length === 0}
      <p class="empty">No boards are open. Each game you start or open gets a board of its own, here or in a window.</p>
    {:else}
      <ul class="windows" aria-label="Boards">
        {#each windows as w (w.id)}
          <li>
            <button class="window" class:waiting={w.yourMove} onclick={() => onWindow(w)} aria-label="Show {w.title}">
              <span class="mini"><MiniBoard position={w.view.position} {theme} flipped={false} /></span>
              <span class="about">
                <span class="title">{w.title}</span>
                {#if w.yourMove}<span><span class="badge">Your move</span></span>{/if}
                <span class="meta">{w.kind}{docked.includes(w.id) ? '' : ' · own window'}</span>
                {#if w.clocks}
                  <span class="meta clocks">
                    <span class="dot gold"></span>{w.clocks.gold ?? '–'}
                    <span class="dot silver"></span>{w.clocks.silver ?? '–'}
                  </span>
                {/if}
                {#if w.result}<span class="meta">{w.result}</span>{/if}
              </span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </section>
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
  h2 {
    margin: 0 0 10px;
    font-size: 15px;
  }
  section + section {
    margin-top: 24px;
  }
  .attention {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 12px;
    margin-bottom: 18px;
    border: 1px solid var(--accent);
    border-radius: 10px;
    background: var(--panel);
    font-size: 13px;
  }
  .actions {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: 10px;
  }
  .action {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 14px;
    text-align: left;
    border-radius: 10px;
  }
  .action-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 38px;
    height: 38px;
    flex: none;
    border-radius: 8px;
    background: var(--hover);
    color: var(--accent);
  }
  .action.primary .action-icon {
    background: rgba(255, 255, 255, 0.18);
    color: inherit;
  }
  .action-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .action-label {
    font-weight: 600;
    font-size: 14px;
  }
  .action-about {
    font-size: 12px;
    color: var(--muted);
  }
  .action.primary .action-about {
    color: inherit;
    opacity: 0.85;
  }
  .windows {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    gap: 10px;
  }
  .window {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    padding: 8px;
    text-align: left;
    border-radius: 10px;
  }
  .window.waiting {
    border-color: var(--accent);
  }
  .mini {
    flex: none;
    width: 88px;
    height: 88px;
  }
  .about {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }
  .title {
    font-weight: 600;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .clocks {
    display: flex;
    align-items: center;
    gap: 4px;
    font-variant-numeric: tabular-nums;
  }
  .clocks .dot.silver {
    margin-left: 8px;
  }
</style>
