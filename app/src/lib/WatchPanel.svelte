<script lang="ts">
  import { api } from './api';
  import { turnTimeLeft } from './clock';
  import type { SessionView } from './bindings/SessionView';
  import type { WatchView } from './bindings/WatchView';

  interface Props {
    watch: WatchView;
    view: SessionView;
    run: (p: Promise<unknown>) => Promise<boolean>;
    /** When `view` arrived and the time now (`performance.now()`), for
     * the running clock. */
    receivedAt: number;
    now: number;
    /** Leaves the finished game for a new open one like it, as `side`. */
    onNewGame?: (side: 'gold' | 'silver', timeControl: string, rated: boolean) => void;
  }
  let { watch, view, run, receivedAt, now, onNewGame }: Props = $props();

  /** The side whose clock has run out, while the server hasn't ended the
   * game yet: it flags time itself, but only checks every few seconds. */
  const flagged = $derived.by(() => {
    const clock = view.clock;
    if (!clock?.running || view.result || watch.state === 'ended') return null;
    return turnTimeLeft(clock, now - receivedAt) <= 0 ? clock.running : null;
  });

  const sideName = (c: 'gold' | 'silver') => (c === 'gold' ? 'Gold' : 'Silver');
  let takeback = $derived(view.takeback);
  let playing = $derived(watch.side != null && watch.state !== 'ended' && !watch.waiting);
  /** Resign was clicked once; the second click resigns. */
  let confirmResign = $state(false);
  $effect(() => {
    if (!playing) confirmResign = false;
  });

  async function resign() {
    confirmResign = false;
    await run(api.resignGameroomGame());
  }

  const STATES: Record<WatchView['state'], string> = {
    following: 'Live',
    reconnecting: 'Reconnecting…',
    ended: 'Game over',
    stopped: 'Stopped',
    failed: 'Stopped: error',
  };
</script>

<section class="watch" aria-label="arimaa.com game">
  <div class="status">
    <span class="state {watch.state}"></span>
    <span>arimaa.com {watch.postal ? 'postal game' : 'game'} {watch.gid}</span>
    <span class="label">
      {watch.waiting && watch.state === 'following' ? 'Waiting' : STATES[watch.state]}{#if watch.finishedId}<span title="The game's permanent arimaa.com id"
          >&nbsp;· #{watch.finishedId}</span
        >{/if}
    </span>
  </div>
  {#if watch.event}<div class="note">{watch.event}</div>{/if}
  {#if watch.detail}<div class="note">{watch.detail}</div>{/if}
  {#if playing && watch.side && watch.away[watch.side === 'gold' ? 1 : 0]}
    <div class="note" role="status">Your opponent isn't at the table; their clock keeps running.</div>
  {/if}
  {#if flagged}
    <div class="note" role="status">
      {sideName(flagged)}'s time is up; waiting for arimaa.com to end the game
    </div>
  {/if}
  {#if watch.refused}<div class="note warn" role="alert">{watch.refused}</div>{/if}
  {#if playing && view.sent != null}<div class="note" role="status">Sending your move…</div>{/if}
  {#if watch.waiting && watch.state !== 'stopped'}
    <div class="takeback" role="status">
      <span>Waiting for an opponent to sit</span>
      <span class="actions">
        <button title="Take the game off the gameroom's open games" onclick={() => run(api.cancelGameroomGame(watch.gid))}
          >Cancel game</button
        >
      </span>
    </div>
    {#if watch.side === 'gold'}
      <div class="note">Your setup goes to the server once they do.</div>
    {/if}
  {:else if takeback}
    <div class="takeback" role="status">
      {#if takeback.by === watch.side}
        <span>{takeback.shown ? 'Takeback asked; waiting for an answer' : 'Asking for a takeback…'}</span>
      {:else if takeback.answer != null}
        <span>{takeback.answer ? 'Accepting' : 'Declining'} the takeback…</span>
      {:else}
        <span>{sideName(takeback.by)} asks for a takeback</span>
        {#if playing}
          <span class="actions">
            <button onclick={() => run(api.answerTakeback(true))}>Accept</button>
            <button onclick={() => run(api.answerTakeback(false))}>Decline</button>
          </span>
        {/if}
      {/if}
    </div>
  {/if}
  {#if playing}
    <div class="takeback">
      {#if confirmResign}
        <span>Resign this game?</span>
        <span class="actions">
          <button class="danger" onclick={resign}>Resign</button>
          <button onclick={() => (confirmResign = false)}>Keep playing</button>
        </span>
      {:else}
        {#if !takeback}
          <button
            disabled={!view.canAskTakeback}
            title="Ask your opponent to take back your last move"
            onclick={() => run(api.requestTakeback())}>Ask for takeback</button
          >
        {/if}
        <span class="actions">
          <button title="Give up the game" onclick={() => (confirmResign = true)}>Resign</button>
        </span>
      {/if}
    </div>
  {/if}
  {#if onNewGame && watch.state === 'ended' && watch.side && watch.timeControl}
    {@const tc = watch.timeControl}
    {@const title = `An open game on arimaa.com, ${tc}${watch.rated ? ', rated' : ''}, for anyone to join (leaves this table)`}
    <div class="takeback">
      <span>New game</span>
      <span class="actions">
        <button {title} onclick={() => onNewGame('gold', tc, watch.rated)}>as Gold</button>
        <button {title} onclick={() => onNewGame('silver', tc, watch.rated)}>as Silver</button>
      </span>
    </div>
  {/if}
</section>

<style>
  .watch {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 13px;
  }
  .status {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .label {
    margin-left: auto;
    color: var(--muted);
    font-size: 12px;
  }
  .state {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--muted);
  }
  .state.following {
    background: #3fb950;
  }
  .state.reconnecting {
    background: #d29922;
  }
  .state.failed {
    background: var(--warn);
  }
  .takeback {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
  }
  .actions {
    display: flex;
    gap: 6px;
    margin-left: auto;
  }
  .note {
    font-size: 12px;
    color: var(--muted);
  }
  .note.warn {
    color: var(--warn);
  }
  .danger {
    color: var(--warn);
  }
</style>
