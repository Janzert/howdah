<script lang="ts">
  // A recently finished game, with Open.
  import type { RecentGameView } from '../bindings/RecentGameView';
  import { recentMeta } from './format';
  import Matchup from './Matchup.svelte';
  import { gameroom } from './store.svelte';

  let { game: g }: { game: RecentGameView } = $props();
</script>

<li class="pick">
  <button onclick={() => gameroom.open(g.gid)} disabled={gameroom.busy} title="Open game {g.gid}">
    <span class="row-text">
      <span class="row-line">
        <Matchup gold={g.gold} silver={g.silver} goldRating={g.goldRating} silverRating={g.silverRating} />
      </span>
      <span class="meta">{recentMeta(g)}</span>
    </span>
    <span class="action">Open</span>
  </button>
</li>
