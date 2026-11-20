<script lang="ts">
  // "● gold 1650 vs ● silver 1720", for the lobby's game rows. With a
  // player's id, their name opens their page.
  import { showPlayerPage } from './player.svelte';

  interface Props {
    gold: string | null;
    silver: string | null;
    goldRating?: number | string | null;
    silverRating?: number | string | null;
    goldId?: string | null;
    silverId?: string | null;
    /** Shown for a missing player. */
    empty?: string;
  }
  let {
    gold,
    silver,
    goldRating = null,
    silverRating = null,
    goldId = null,
    silverId = null,
    empty = '?',
  }: Props = $props();
</script>

{#snippet player(name: string | null, id: string | null)}
  {#if name && id}
    <button class="link name player" onclick={() => showPlayerPage({ id, username: name })} title="{name}'s page"
      >{name}</button
    >
  {:else}
    <span class="name">{name ?? empty}</span>
  {/if}
{/snippet}

<span class="dot gold"></span>{@render player(gold, goldId)}
{#if goldRating}<span class="rating">{goldRating}</span>{/if}
<span class="vs">vs</span>
<span class="dot silver"></span>{@render player(silver, silverId)}
{#if silverRating}<span class="rating">{silverRating}</span>{/if}
