<script lang="ts">
  // A game being played, with Watch.
  import type { LiveGameView } from '../bindings/LiveGameView';
  import { gameMeta } from './format';
  import Matchup from './Matchup.svelte';
  import { gameroom } from './store.svelte';

  let { game: g }: { game: LiveGameView } = $props();
</script>

<li class="pick">
  <button onclick={() => gameroom.open(g.gid)} disabled={gameroom.busy} title="Watch game {g.gid}">
    <span class="row-text">
      <span class="row-line"><Matchup gold={g.gold} silver={g.silver} /></span>
      <span class="meta">{gameMeta(g)}</span>
    </span>
    <span class="action">Watch</span>
  </button>
</li>
