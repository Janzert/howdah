<script lang="ts">
  import { api } from './api';
  import type { SessionView } from './bindings/SessionView';
  import type { WatchView } from './bindings/WatchView';

  interface Props {
    watch: WatchView;
    view: SessionView;
    run: (p: Promise<unknown>) => Promise<boolean>;
  }
  let { watch, view, run }: Props = $props();

  const sideName = (c: 'gold' | 'silver') => (c === 'gold' ? 'Gold' : 'Silver');
  let takeback = $derived(view.takeback);
  let playing = $derived(watch.side != null && watch.state !== 'ended' && !watch.waiting);

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
    <span>arimaa.com game {watch.gid}</span>
    <span class="label">
      {watch.waiting && watch.state === 'following' ? 'Waiting' : STATES[watch.state]}{#if watch.finishedId}<span title="The game's permanent arimaa.com id"
          >&nbsp;· #{watch.finishedId}</span
        >{/if}
    </span>
  </div>
  {#if watch.event}<div class="note">{watch.event}</div>{/if}
  {#if watch.detail}<div class="note">{watch.detail}</div>{/if}
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
  {:else if playing}
    <div class="takeback">
      <button
        disabled={!view.canAskTakeback}
        title="Ask your opponent to take back your last move"
        onclick={() => run(api.requestTakeback())}>Ask for takeback</button
      >
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
</style>
