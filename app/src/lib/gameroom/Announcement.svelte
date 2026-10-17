<script lang="ts">
  // One of the gameroom's announcements, read into blocks of styled spans
  // by the backend (no server HTML reaches the page). Links open in the
  // system's browser.
  import type { AnnouncementView } from '../bindings/AnnouncementView';
  import { announcementDate } from './format';
  import { gameroom } from './store.svelte';

  let { announcement: a, older = false }: { announcement: AnnouncementView; older?: boolean } = $props();
</script>

<aside class="announcement" class:older aria-label="{older ? 'Older announcement' : 'Announcement'} from arimaa.com">
  {#each a.blocks as b, i (i)}
    <p class:item={b.item}>
      {#each b.spans as s, j (j)}
        {#if s.link}
          <a href={s.link} class:bold={s.bold} class:italic={s.italic} onclick={(e) => gameroom.openLink(e, s.link!)}
            >{s.text}</a
          >
        {:else}
          <span class:bold={s.bold} class:italic={s.italic}>{s.text}</span>
        {/if}
      {/each}
    </p>
  {/each}
  <div class="footer">
    {announcementDate(a)}
    {#if !older}<button class="subtle" onclick={() => gameroom.hideAnnouncement(a)}>Hide</button>{/if}
  </div>
</aside>

<style>
  .announcement {
    font-size: 13px;
    padding: 8px 12px;
    border: 1px solid var(--border);
    border-left: 3px solid var(--accent);
    border-radius: 8px;
    background: var(--panel);
  }
  .announcement.older {
    border-left-color: var(--border);
  }
  p {
    margin: 4px 0;
    white-space: pre-line;
  }
  p.item {
    display: list-item;
    margin-left: 1.2em;
  }
  .bold {
    font-weight: 600;
  }
  .italic {
    font-style: italic;
  }
  a {
    color: var(--accent);
  }
  .footer {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 11px;
    color: var(--muted);
  }
</style>
