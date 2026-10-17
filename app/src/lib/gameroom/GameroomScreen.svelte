<script lang="ts">
  // The lobby window's arimaa.com panel: the login, then the overview
  // (what needs the user and the top of each list), with buttons opening
  // the other arimaa.com panels (`GameroomPanel`), which the sidebar then
  // lists under arimaa.com. State lives in the store, so it outlasts the
  // panels.
  import Icon from '../Icon.svelte';
  import GameroomFrame from './GameroomFrame.svelte';
  import Overview from './Overview.svelte';
  import { GAMEROOM_PANELS, gameroom } from './store.svelte';
</script>

<GameroomFrame title="arimaa.com">
  <nav class="panels" aria-label="arimaa.com panels">
    {#each GAMEROOM_PANELS as p (p.id)}
      <button class="icon" onclick={() => gameroom.show(p.id)}><Icon name={p.icon} size={16} /> {p.label}</button>
    {/each}
  </nav>
  <Overview />
</GameroomFrame>

<style>
  .panels {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin: -4px 0 16px;
  }
</style>
