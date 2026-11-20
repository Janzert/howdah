<script lang="ts">
  // A game from a page of finished games, with Open.
  import type { PastGameView } from '../bindings/PastGameView';
  import { pastMeta } from './format';
  import Matchup from './Matchup.svelte';
  import { gameroom } from './store.svelte';

  let { game: g }: { game: PastGameView } = $props();
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
    <span class="meta">{pastMeta(g)}</span>
  </span>
  <button
    class="open"
    onclick={() => gameroom.openFinished(g.gid, g.comments)}
    disabled={gameroom.busy}
    title="Open game {g.gid}">Open</button
  >
</li>
