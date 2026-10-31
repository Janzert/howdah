<script lang="ts">
  // A game from a page of finished games, with Open.
  import type { PastGameView } from '../bindings/PastGameView';
  import { pastMeta } from './format';
  import Matchup from './Matchup.svelte';
  import { gameroom } from './store.svelte';

  let { game: g }: { game: PastGameView } = $props();
</script>

<li class="pick">
  <button onclick={() => gameroom.open(g.gid)} disabled={gameroom.busy} title="Open game {g.gid}">
    <span class="row-text">
      <span class="row-line"><Matchup gold={g.gold} silver={g.silver} /></span>
      <span class="meta">{pastMeta(g)}</span>
    </span>
    <span class="action">Open</span>
  </button>
</li>
