<script lang="ts">
  import { api, errorMessage } from './api';
  import type { SessionView } from './bindings/SessionView';
  import type { WatchView } from './bindings/WatchView';

  interface Props {
    watch: WatchView;
    view: SessionView;
  }
  let { watch, view }: Props = $props();

  let text = $state('');
  let sending = $state(false);
  let error = $state<string | null>(null);
  let log: HTMLOListElement | undefined = $state();

  /** Only players can write; the table goes away once the game is over. */
  const canSend = $derived(watch.side != null && watch.state !== 'ended' && watch.state !== 'failed');

  const name = (side: 'gold' | 'silver' | null) =>
    side == null ? '' : (view.players?.[side].name ?? (side === 'gold' ? 'Gold' : 'Silver'));

  // Keep the newest line in view.
  $effect(() => {
    void watch.chat.length;
    if (log) log.scrollTop = log.scrollHeight;
  });

  async function send(e: SubmitEvent) {
    e.preventDefault();
    if (!text.trim() || sending) return;
    sending = true;
    error = null;
    try {
      await api.sendGameroomChat(text);
      text = '';
    } catch (err) {
      error = errorMessage(err);
    } finally {
      sending = false;
    }
  }
</script>

<section class="chat" aria-label="Game chat">
  <ol class="lines" bind:this={log} aria-live="polite">
    {#each watch.chat as line, i (i)}
      <li>
        {#if line.side}<span class="dot {line.side}"></span><span class="who">{name(line.side)}</span>{/if}
        {#if line.label}<span class="label">{line.label}</span>{/if}
        <span class="text">{line.text}</span>
      </li>
    {:else}
      <li class="empty">{canSend ? 'Chat with your opponent here.' : 'No chat.'}</li>
    {/each}
  </ol>
  {#if canSend}
    <form onsubmit={send}>
      <input bind:value={text} placeholder="Message" aria-label="Chat message" autocomplete="off" />
      <button type="submit" disabled={sending || !text.trim()}>Send</button>
    </form>
  {/if}
  {#if error}<div class="error" role="alert">{error}</div>{/if}
</section>

<style>
  .chat {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 13px;
    flex: none;
  }
  .lines {
    list-style: none;
    margin: 0;
    padding: 4px 8px;
    height: 6.5em;
    overflow-y: auto;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--panel);
  }
  .lines li {
    display: flex;
    align-items: baseline;
    gap: 6px;
    padding: 1px 0;
  }
  .empty {
    color: var(--muted);
  }
  .who {
    font-weight: 600;
    white-space: nowrap;
  }
  .label {
    color: var(--muted);
    font-size: 11px;
  }
  .text {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    min-width: 0;
  }
  .dot {
    flex: none;
    align-self: center;
    width: 9px;
    height: 9px;
    border-radius: 50%;
    border: 1px solid rgba(0, 0, 0, 0.4);
  }
  .dot.gold {
    background: #e3b23c;
  }
  .dot.silver {
    background: #c9ced6;
  }
  form {
    display: flex;
    gap: 6px;
  }
  input {
    flex: 1;
    min-width: 0;
  }
  .error {
    color: var(--warn);
    font-size: 12px;
  }
</style>
