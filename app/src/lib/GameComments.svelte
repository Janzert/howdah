<script lang="ts">
  // The comments people left on a finished arimaa.com game, below the
  // board where a game's chat is. Read only. They're fetched when the
  // board opened the game from arimaa.com (unless its list said it has
  // none); otherwise, as for a record from a file, only when asked for.
  import { api, errorMessage } from './api';
  import type { GameCommentView } from './bindings/GameCommentView';
  import type { SessionView } from './bindings/SessionView';
  import { commentCount } from './gameroom/format';

  interface Props {
    /** The game's permanent id. */
    gid: string;
    view: SessionView;
    /** The arimaa.com game the board opened on, if it did. */
    opened?: { gid: string; comments?: number };
  }
  let { gid, view, opened }: Props = $props();

  let comments = $state<GameCommentView[] | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);

  async function load() {
    if (loading) return;
    loading = true;
    error = null;
    try {
      comments = await api.gameroomGameComments(gid);
    } catch (e) {
      error = errorMessage(e);
    } finally {
      loading = false;
    }
  }

  // svelte-ignore state_referenced_locally
  const known = opened?.gid === gid ? opened.comments : undefined;
  // svelte-ignore state_referenced_locally
  if (opened?.gid === gid && known !== 0) void load();

  /** The side a comment's writer played, if they played. */
  function side(author: string): 'gold' | 'silver' | null {
    const is = (name: string | null | undefined) => name?.toLowerCase() === author.toLowerCase();
    const names = view.players ? [view.players.gold.name, view.players.silver.name] : view.tagNames;
    return is(names?.[0]) ? 'gold' : is(names?.[1]) ? 'silver' : null;
  }
</script>

<section class="comments" aria-label="Comments on arimaa.com">
  {#if comments?.length}
    <ol class="lines">
      {#each comments as c, i (i)}
        <li>
          <div class="by">
            {#if side(c.author)}<span class="dot {side(c.author)}"></span>{/if}
            <span class="who">{c.author}</span>
            {#if c.written}<span class="when">{c.written}</span>{/if}
          </div>
          <div class="text">{c.text}</div>
        </li>
      {/each}
    </ol>
  {:else}
    <div class="note" role="status">
      {#if loading}
        Getting the comments from arimaa.com…
      {:else if error}
        <span class="error">Couldn't get the comments: {error}</span>
        <button onclick={load}>Try again</button>
      {:else if comments}
        No comments on arimaa.com.
      {:else}
        {known === 0 ? 'No comments on arimaa.com.' : 'Comments on arimaa.com'}
        <button onclick={load}>{known === 0 ? 'Check again' : 'Show'}</button>
      {/if}
    </div>
  {/if}
  {#if comments?.length}
    <div class="note">
      {commentCount(comments.length)} on arimaa.com
      <button onclick={load} disabled={loading}>Refresh</button>
      {#if error}<span class="error">{error}</span>{/if}
    </div>
  {/if}
</section>

<style>
  .comments {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: calc(var(--chat-font) - 1px);
    flex: none;
  }
  .lines {
    list-style: none;
    margin: 0;
    padding: 4px 8px;
    max-height: 9em;
    overflow-y: auto;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--panel);
  }
  .lines li {
    padding: 3px 0;
  }
  .lines li + li {
    border-top: 1px solid var(--border);
  }
  .by {
    display: flex;
    align-items: baseline;
    gap: 6px;
  }
  .who {
    font-weight: 600;
  }
  .when {
    color: var(--muted);
    font-size: 0.85em;
  }
  .text {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
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
  .note {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--muted);
    font-size: 12px;
  }
  .note button {
    font-size: inherit;
    padding: 1px 8px;
  }
  .error {
    color: var(--warn);
  }
</style>
