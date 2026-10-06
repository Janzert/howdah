<script lang="ts">
  import type { WatchView } from './bindings/WatchView';

  interface Props {
    watch: WatchView;
  }
  let { watch }: Props = $props();

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
      {STATES[watch.state]}{#if watch.finishedId}<span title="The game's permanent arimaa.com id"
          >&nbsp;· #{watch.finishedId}</span
        >{/if}
    </span>
  </div>
  {#if watch.event}<div class="note">{watch.event}</div>{/if}
  {#if watch.detail}<div class="note">{watch.detail}</div>{/if}
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
  .note {
    font-size: 12px;
    color: var(--muted);
  }
</style>
