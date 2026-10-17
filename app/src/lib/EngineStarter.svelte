<script lang="ts">
  // Home's first-run card: with no engine but the bundled test engine,
  // offers the suggested engines (Sharp, OpFor) to download and install
  // in one click each, as the Engines panel's Download does.
  import { onMount } from 'svelte';
  import { api, errorMessage } from './api';
  import type { EngineSpec } from './bindings/EngineSpec';
  import type { SuggestedEngine } from './bindings/SuggestedEngine';
  import Icon from './Icon.svelte';

  interface Props {
    engines: EngineSpec[];
    /** After an install, to reload the engine list. */
    onChanged: () => void;
    /** Opens the Engines panel. */
    onEngines: () => void;
  }
  let { engines, onChanged, onEngines }: Props = $props();

  /** The bundled random mover, which the tests use. */
  const isTestEngine = (e: EngineSpec) => /aei-test-engine(\.exe)?$/.test(e.program);
  const needed = $derived(engines.every(isTestEngine));

  let suggested = $state<SuggestedEngine[]>([]);
  let working = $state<string | null>(null);
  let error = $state<string | null>(null);

  onMount(async () => {
    try {
      suggested = (await api.engineCatalog()).suggested;
    } catch {
      suggested = [];
    }
  });

  async function download(s: SuggestedEngine) {
    working = s.url;
    error = null;
    try {
      const before = await api.engineCatalog();
      const known = new Set(before.manifests.map((m) => m.id));
      const after = await api.addEngineManifest({ url: s.url });
      const added = after.manifests.find((m) => !known.has(m.id) || m.source === s.url);
      if (!added?.downloadable) throw new Error(`${s.name} has no download for this computer`);
      await api.installEngine(added.id);
      suggested = (await api.engineCatalog()).suggested;
      onChanged();
    } catch (e) {
      error = errorMessage(e);
    }
    working = null;
  }
</script>

{#if needed && suggested.length > 0}
  <section class="card starter" aria-labelledby="starter-title">
    <div class="card-head">
      <Icon name="cpu" />
      <h2 id="starter-title">Get an engine</h2>
    </div>
    <p class="hint">
      Engines play against you and analyse positions. Download one to start; more, and engines of your own, are in
      Engines.
    </p>
    <div class="buttons">
      {#each suggested as s (s.url)}
        <button class="primary" onclick={() => download(s)} disabled={working != null}>
          {working === s.url ? `Downloading ${s.name}…` : `Download ${s.name}`}
        </button>
      {/each}
      <button onclick={onEngines}>Engines</button>
    </div>
    {#if error}<p class="error" role="alert">{error}</p>{/if}
  </section>
{/if}

<style>
  .starter {
    margin-bottom: 18px;
    border-color: var(--accent);
  }
  .card-head {
    color: var(--accent);
  }
  .card-head h2 {
    color: var(--text);
  }
  .buttons {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 10px;
  }
  .error {
    margin-top: 8px;
  }
</style>
