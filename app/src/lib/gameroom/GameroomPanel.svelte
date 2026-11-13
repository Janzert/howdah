<script lang="ts">
  // One of the arimaa.com panels opened from the overview (Bots, Watch,
  // Finished games, Players, a player's page, Chat room, Chat archive), listed under arimaa.com in the sidebar until
  // closed. Each player's page is a panel of its own.
  import { untrack } from 'svelte';
  import BotsTab from './BotsTab.svelte';
  import ChatArchiveTab from './ChatArchiveTab.svelte';
  import ChatRoomTab from './ChatRoomTab.svelte';
  import FinishedTab from './FinishedTab.svelte';
  import GameroomFrame from './GameroomFrame.svelte';
  import PlayersTab from './PlayersTab.svelte';
  import PlayerTab from './PlayerTab.svelte';
  import { playerPages } from './player.svelte';
  import { GAMEROOM_PANELS, NO_LOGIN_PANELS, type GameroomTab, playerPanelId } from './store.svelte';
  import WatchTab from './WatchTab.svelte';

  let { panel, onClose }: { panel: GameroomTab; onClose: () => void } = $props();
  // The panel never changes (the lobby keys it), so neither does its page.
  const page = untrack(() => {
    const playerId = playerPanelId(panel);
    return playerId != null ? playerPages.page(playerId) : null;
  });
  const label = $derived(page ? page.player.username : (GAMEROOM_PANELS.find((p) => p.id === panel)?.label ?? panel));
</script>

<GameroomFrame title="arimaa.com · {label}" {onClose} noLogin={NO_LOGIN_PANELS.includes(panel)}>
  {#if page}
    <PlayerTab {page} />
  {:else if panel === 'bots'}
    <BotsTab />
  {:else if panel === 'watch'}
    <WatchTab />
  {:else if panel === 'finished'}
    <FinishedTab />
  {:else if panel === 'chat'}
    <ChatRoomTab />
  {:else if panel === 'archive'}
    <ChatArchiveTab />
  {:else}
    <PlayersTab />
  {/if}
</GameroomFrame>
