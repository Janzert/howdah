<script lang="ts">
  // "● gold 1650 vs ● silver 1720", for the lobby's game rows.
  interface Props {
    gold: string | null;
    silver: string | null;
    goldRating?: number | string | null;
    silverRating?: number | string | null;
    /** Shown for a missing player. */
    empty?: string;
  }
  let { gold, silver, goldRating = null, silverRating = null, empty = '?' }: Props = $props();
</script>

<span class="dot gold"></span><span class="name">{gold ?? empty}</span>
{#if goldRating}<span class="rating">{goldRating}</span>{/if}
<span class="vs">vs</span>
<span class="dot silver"></span><span class="name">{silver ?? empty}</span>
{#if silverRating}<span class="rating">{silverRating}</span>{/if}
