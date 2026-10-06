<script lang="ts">
  import { api, errorMessage } from './api';
  import type { EngineCatalogView } from './bindings/EngineCatalogView';
  import type { EngineOption } from './bindings/EngineOption';
  import type { EngineSpec } from './bindings/EngineSpec';
  import type { InstalledFrom } from './bindings/InstalledFrom';
  import type { ManifestView } from './bindings/ManifestView';

  interface Props {
    /** Called after any change so the parent can reload the list. */
    onChanged: () => void;
    onClose: () => void;
  }
  let { onChanged, onClose }: Props = $props();

  let engines = $state<EngineSpec[]>([]);
  let editing = $state<{
    id: string;
    name: string;
    program: string;
    args: string;
    workingDir: string;
    options: string;
    /** Kept as it was, so an installed engine stays tied to its manifest. */
    installed?: InstalledFrom;
  } | null>(null);
  let catalog = $state<EngineCatalogView | null>(null);
  /** What's being done to a manifest (by id or url), and the last error. */
  let working = $state<string | null>(null);
  let catalogError = $state<string | null>(null);
  let adding = $state(false);
  let manifestUrl = $state('');
  let status = $state<{ ok: boolean; text: string } | null>(null);
  let busy = $state(false);
  let dialog: HTMLDialogElement;

  $effect(() => {
    dialog.showModal();
    reload();
  });

  async function reload() {
    engines = await api.listEngines();
    catalog = await api.engineCatalog();
  }

  /** Runs a catalog action, showing what's going on and any error. */
  async function act(key: string, f: () => Promise<unknown>) {
    working = key;
    catalogError = null;
    try {
      await f();
      await reload();
      onChanged();
      return true;
    } catch (e) {
      catalogError = errorMessage(e);
      return false;
    } finally {
      working = null;
    }
  }

  /** A suggested engine: fetch its manifest, then install it. */
  async function getSuggested(url: string) {
    await act(url, async () => {
      const before = new Set(catalog?.manifests.map((m) => m.id));
      const after = await api.addEngineManifest({ url });
      const added = after.manifests.find((m) => !before.has(m.id) || m.source === url);
      if (added?.downloadable) await api.installEngine(added.id);
    });
  }

  async function addFromUrl(e: SubmitEvent) {
    e.preventDefault();
    if (await act('add', () => api.addEngineManifest({ url: manifestUrl.trim() }))) {
      manifestUrl = '';
      adding = false;
    }
  }

  async function addFromFile(e: Event) {
    const input = e.target as HTMLInputElement;
    const file = input.files?.[0];
    input.value = '';
    if (!file) return;
    const text = await file.text();
    if (await act('add', () => api.addEngineManifest({ text }))) adding = false;
  }

  /** What the manifest's install button says, if it has one. */
  function installLabel(m: ManifestView): string | null {
    if (!m.downloadable) return null;
    if (m.installedVersion == null) return 'Install';
    return m.installedVersion === m.version ? null : `Update to ${m.version}`;
  }

  function edit(e: EngineSpec | null) {
    status = null;
    editing = e
      ? {
          id: e.id,
          name: e.name,
          program: e.program,
          args: e.args.join(' '),
          workingDir: e.workingDir ?? '',
          options: e.options.map((o) => `${o.name} = ${o.value}`).join('\n'),
          installed: e.installed,
        }
      : { id: '', name: '', program: '', args: '', workingDir: '', options: '' };
  }

  function spec(): EngineSpec {
    const e = editing!;
    return {
      id: e.id,
      name: e.name.trim(),
      program: e.program.trim(),
      args: e.args.split(/\s+/).filter((a) => a.length > 0),
      workingDir: e.workingDir.trim() || null,
      options: parseOptions(e.options),
      installed: e.installed,
    };
  }

  /** `name = value` lines; blank lines are skipped, and a line without `=`
   * is a name with an empty value. */
  function parseOptions(text: string): EngineOption[] {
    return text
      .split('\n')
      .map((line) => line.trim())
      .filter((line) => line.length > 0)
      .map((line) => {
        const i = line.indexOf('=');
        return i < 0 ? { name: line, value: '' } : { name: line.slice(0, i).trim(), value: line.slice(i + 1).trim() };
      });
  }

  async function test() {
    busy = true;
    status = null;
    try {
      const id = await api.testEngine(spec());
      const parts = [id.name ?? 'unnamed', id.version && `version ${id.version}`, id.author && `by ${id.author}`];
      status = { ok: true, text: `OK: ${parts.filter(Boolean).join(', ')} (AEI protocol ${id.protocolVersion})` };
    } catch (e) {
      status = { ok: false, text: errorMessage(e) };
    } finally {
      busy = false;
    }
  }

  async function save() {
    try {
      await api.saveEngine(spec());
      editing = null;
      await reload();
      onChanged();
    } catch (e) {
      status = { ok: false, text: errorMessage(e) };
    }
  }

  async function remove(e: EngineSpec) {
    await api.deleteEngine(e.id);
    await reload();
    onChanged();
  }
</script>

<dialog bind:this={dialog} onclose={onClose}>
  <h2>Engines</h2>
  {#if editing}
    <div class="form">
      <label for="en-name">Name</label>
      <input id="en-name" bind:value={editing.name} placeholder="e.g. Sharp" />
      <label for="en-program">Program</label>
      <input id="en-program" bind:value={editing.program} placeholder="/path/to/engine" />
      <label for="en-args">Arguments</label>
      <input id="en-args" bind:value={editing.args} placeholder="e.g. aei" />
      <label for="en-dir">Working dir</label>
      <input id="en-dir" bind:value={editing.workingDir} placeholder="optional" />
      <label for="en-options">Options</label>
      <textarea id="en-options" rows="3" bind:value={editing.options} placeholder="e.g. threads = 2"></textarea>
    </div>
    <p class="hint">
      The program is run directly, not through a shell. Arguments are split on spaces. Options are one per line,
      as <code>name = value</code>, sent with <code>setoption</code> for games and analysis. Sharp and OpFor get
      what analysis needs without any.
    </p>
    {#if status}<p class:ok={status.ok} class="status">{status.text}</p>{/if}
    <div class="buttons">
      <button onclick={test} disabled={busy}>{busy ? 'Testing…' : 'Test'}</button>
      <span class="spacer"></span>
      <button onclick={() => (editing = null)}>Cancel</button>
      <button class="primary" onclick={save}>Save</button>
    </div>
  {:else}
    {#if engines.length === 0}
      <p class="hint">No engines yet. Add any program that speaks AEI.</p>
    {/if}
    <ul>
      {#each engines as e (e.id)}
        <li>
          <div class="info">
            <strong>{e.name}{#if e.installed}<span class="version"> · {e.installed.version}</span>{/if}</strong>
            <code>{[e.program, ...e.args].join(' ')}</code>
          </div>
          <button onclick={() => edit(e)}>Edit</button>
          <button onclick={() => remove(e)}>Delete</button>
        </li>
      {/each}
    </ul>
    <h3>Engine downloads</h3>
    {#if catalog}
      <ul>
        {#each catalog.manifests as m (m.id)}
          {@const label = installLabel(m)}
          <li>
            <div class="info">
              <strong>
                {m.name}
                <span class="version">
                  {m.version}{#if m.installedVersion}&nbsp;· installed {m.installedVersion}{/if}
                </span>
              </strong>
              {#if m.description}<span class="desc">{m.description}</span>{/if}
              <span class="desc">
                {[m.author, m.license].filter(Boolean).join(' · ')}{#if m.homepage}
                  · <a href={m.homepage} target="_blank" rel="noreferrer">home page</a>{/if}
                {#if !m.downloadable}· no download for this computer ({catalog.platform}){/if}
              </span>
              <code title="Where this manifest came from">{m.source ?? 'from a file'}</code>
            </div>
            {#if working === m.id}
              <span class="desc">Working…</span>
            {:else}
              {#if label}
                <button
                  disabled={working != null}
                  title="Download this release for this computer and add it to your engines"
                  onclick={() => act(m.id, () => api.installEngine(m.id))}>{label}</button
                >
              {/if}
              {#if m.updatable}
                <button
                  disabled={working != null}
                  title="Fetch the newest release's manifest (nothing is installed)"
                  onclick={() => act(m.id, () => api.refreshEngineManifest(m.id))}>Check for update</button
                >
              {/if}
              <button
                disabled={working != null}
                title="Forget this manifest; an engine installed from it stays"
                onclick={() => act(m.id, () => api.removeEngineManifest(m.id))}>Remove</button
              >
            {/if}
          </li>
        {/each}
        {#each catalog.suggested as s (s.url)}
          <li>
            <div class="info">
              <strong>{s.name}</strong>
              <code>{s.url}</code>
            </div>
            {#if working === s.url}
              <span class="desc">Downloading…</span>
            {:else}
              <button disabled={working != null} onclick={() => getSuggested(s.url)}>Download</button>
            {/if}
          </li>
        {/each}
      </ul>
      {#if adding}
        <form class="add" onsubmit={addFromUrl}>
          <input bind:value={manifestUrl} placeholder="https://…/engine.json" aria-label="Manifest address" />
          <button type="submit" disabled={working != null || !manifestUrl.trim()}>Add</button>
          <label class="file">
            <input type="file" accept=".json,application/json" onchange={addFromFile} />
            <span>From a file…</span>
          </label>
          <button type="button" onclick={() => (adding = false)}>Cancel</button>
        </form>
        <p class="hint">
          An engine manifest (<code>engine.json</code>) says where to download an engine for each computer. Engines
          publish one with each release.
        </p>
      {/if}
      {#if catalogError}<p class="status" role="alert">{catalogError}</p>{/if}
    {/if}
    <div class="buttons">
      <button onclick={() => edit(null)}>Add engine…</button>
      <button onclick={() => (adding = true)} disabled={adding}>Add from manifest…</button>
      <span class="spacer"></span>
      <button class="primary" onclick={onClose}>Done</button>
    </div>
  {/if}
</dialog>

<style>
  dialog {
    width: min(620px, 92vw);
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--panel);
    color: var(--text);
    padding: 16px;
  }
  dialog::backdrop {
    background: rgba(0, 0, 0, 0.4);
  }
  h2 {
    margin: 0 0 12px;
    font-size: 16px;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  li {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 0;
    border-bottom: 1px solid var(--border);
  }
  .info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  code {
    font-size: 11px;
    color: var(--muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .form {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 8px 12px;
    align-items: center;
  }
  input,
  textarea {
    font: inherit;
    padding: 4px 8px;
    border: 1px solid var(--border);
    border-radius: 5px;
    background: var(--bg);
    color: var(--text);
  }
  .hint {
    font-size: 12px;
    color: var(--muted);
  }
  .hint code {
    font-size: 11px;
  }
  textarea {
    resize: vertical;
    font-family: ui-monospace, 'DejaVu Sans Mono', monospace;
    font-size: 12px;
  }
  label[for='en-options'] {
    align-self: start;
    padding-top: 4px;
  }
  .status {
    font-size: 13px;
    color: var(--warn);
    word-break: break-word;
  }
  .status.ok {
    color: var(--accent);
  }
  .buttons {
    display: flex;
    gap: 6px;
    margin-top: 12px;
  }
  .spacer {
    flex: 1;
  }
  h3 {
    margin: 16px 0 4px;
    font-size: 13px;
    color: var(--muted);
  }
  .version,
  .desc {
    font-weight: normal;
    font-size: 12px;
    color: var(--muted);
  }
  .add {
    display: flex;
    gap: 6px;
    margin-top: 8px;
  }
  .add input:not([type='file']) {
    flex: 1;
    min-width: 0;
  }
  .file input {
    display: none;
  }
  .file span {
    display: inline-block;
    padding: 4px 8px;
    border: 1px solid var(--border);
    border-radius: 5px;
    cursor: pointer;
  }
</style>
