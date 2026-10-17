<script lang="ts">
  // What every arimaa.com panel has: the header (its title, who's logged
  // in, Refresh, Log out, and Close for a panel opened from the
  // overview), the login form until logged in, and the last error. The
  // lists refresh every 20 s while a panel is shown.
  import type { Snippet } from 'svelte';
  import Icon from '../Icon.svelte';
  import LoginForm from './LoginForm.svelte';
  import { gameroom } from './store.svelte';

  interface Props {
    title: string;
    /** Closes the panel (one opened from the overview). */
    onClose?: () => void;
    /** Beside the title, while logged in. */
    titleExtra?: Snippet;
    children: Snippet;
  }
  let { title, onClose, titleExtra, children }: Props = $props();

  $effect(() => {
    if (gameroom.user) return gameroom.autoRefresh();
  });
</script>

<div class="screen">
  <header>
    <h1>{title}</h1>
    {#if gameroom.user && titleExtra}{@render titleExtra()}{/if}
    <span class="spacer"></span>
    {#if gameroom.user}
      <span class="who">Logged in as <strong>{gameroom.user}</strong></span>
      <button class="icon" onclick={() => gameroom.refresh()} disabled={gameroom.busy} title="Refresh the lists"
        aria-label="Refresh"><Icon name="refresh" size={16} /></button
      >
      {#if !onClose}
        <button class="subtle" onclick={() => gameroom.logout()} disabled={gameroom.busy}>Log out</button>
      {/if}
    {/if}
    {#if onClose}
      <button onclick={onClose} title="Close this panel">Close</button>
    {/if}
  </header>

  {#if gameroom.user === undefined}
    <p class="empty">…</p>
  {:else if gameroom.user === null}
    <LoginForm />
  {:else}
    {#if gameroom.error && gameroom.newGame == null}
      <div class="banner" role="alert">
        <span>{gameroom.error}</span>
        <span class="spacer"></span>
        <button class="subtle" onclick={() => (gameroom.error = null)} aria-label="Dismiss">
          <Icon name="close" size={14} />
        </button>
      </div>
    {/if}
    {@render children()}
  {/if}
</div>

<style>
  header {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 14px;
    min-height: 30px;
  }
  h1 {
    margin: 0;
    font-size: 20px;
  }
  .who {
    font-size: 13px;
    color: var(--muted);
  }
  .who strong {
    color: var(--text);
  }
  header button.icon {
    padding: 4px 6px;
  }
  .banner {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    margin-bottom: 14px;
    border: 1px solid var(--warn);
    border-radius: 8px;
    color: var(--warn);
    font-size: 13px;
  }
  .banner button {
    display: flex;
    padding: 3px;
  }
</style>
