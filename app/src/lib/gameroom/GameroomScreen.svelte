<script lang="ts">
  // The lobby window's arimaa.com panel: the login, then the overview
  // (what needs the user and the top of each list), whose sections have
  // buttons opening the other arimaa.com panels (`GameroomPanel`), which
  // the sidebar then lists under arimaa.com. State lives in the store, so
  // it outlasts the panels.
  import GameroomFrame from './GameroomFrame.svelte';
  import Overview from './Overview.svelte';
</script>

<GameroomFrame title="arimaa.com">
  <Overview />
</GameroomFrame>
