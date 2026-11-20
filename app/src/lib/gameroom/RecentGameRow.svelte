<script lang="ts">
  // A recently finished game, with Open.
  import { clock } from '../clock.svelte';
  import type { RecentGameView } from '../bindings/RecentGameView';
  import { recentMeta } from './format';
  import Matchup from './Matchup.svelte';
  import { gameroom } from './store.svelte';

  let { game: g }: { game: RecentGameView } = $props();
</script>

<li class="pick split">
  <span class="row-text">
    <span class="row-line">
      <Matchup
        gold={g.gold}
        silver={g.silver}
        goldRating={g.goldRating}
        silverRating={g.silverRating}
        goldId={g.goldId}
        silverId={g.silverId}
      />
    </span>
    <span class="meta">{recentMeta(g, clock.options)}</span>
  </span>
  <button class="open" onclick={() => gameroom.openFinished(g.gid)} disabled={gameroom.busy} title="Open game {g.gid}"
    >Open</button
  >
</li>
