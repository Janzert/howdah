<script lang="ts">
  // One of the arimaa.com panels opened from the overview (Bots, Watch,
  // Finished games, Players, Chat room), listed under arimaa.com in the sidebar until
  // closed.
  import BotsTab from './BotsTab.svelte';
  import ChatRoomTab from './ChatRoomTab.svelte';
  import FinishedTab from './FinishedTab.svelte';
  import GameroomFrame from './GameroomFrame.svelte';
  import PlayersTab from './PlayersTab.svelte';
  import { GAMEROOM_PANELS, type GameroomTab } from './store.svelte';
  import WatchTab from './WatchTab.svelte';

  let { panel, onClose }: { panel: GameroomTab; onClose: () => void } = $props();
  const label = $derived(GAMEROOM_PANELS.find((p) => p.id === panel)?.label ?? panel);
</script>

<GameroomFrame title="arimaa.com · {label}" {onClose}>
  {#if panel === 'bots'}
    <BotsTab />
  {:else if panel === 'watch'}
    <WatchTab />
  {:else if panel === 'finished'}
    <FinishedTab />
  {:else if panel === 'chat'}
    <ChatRoomTab />
  {:else}
    <PlayersTab />
  {/if}
</GameroomFrame>
