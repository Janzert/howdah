<script lang="ts">
  // The keyboard and mouse help: a dialog in a game window (`?`), the Help
  // panel in the lobby window (`panel`).
  import { api } from './api';
  import { keyName, shortcutFor, shortcutGroups } from './shortcuts';

  let { onClose, panel = false }: { onClose?: () => void; panel?: boolean } = $props();

  let dialog = $state<HTMLDialogElement>();
  const groups = shortcutGroups();

  const REPO = 'https://github.com/Janzert/howdah';
  let version = $state<string | null>(null);
  let copied = $state(false);
  $effect(() => {
    if (panel) api.appVersion().then((v) => (version = v));
  });
  const copyVersion = async () => {
    await navigator.clipboard.writeText(`Howdah ${version}`);
    copied = true;
  };

  $effect(() => {
    dialog?.showModal();
  });

  // `?` closes the help as well as opening it (Esc closes any dialog).
  function onkeydown(e: KeyboardEvent) {
    if (shortcutFor(e)?.id === 'help') {
      e.preventDefault();
      dialog?.close();
    }
  }

  /** The lobby window's keys (`lobbyKey` in lib/frames.ts), which also
   * work from a board docked there. */
  const lobby: { keys: string[]; effect: string }[] = [
    { keys: ['Alt+↑', 'Alt+↓'], effect: 'Previous or next entry in the sidebar' },
    { keys: ['Alt+1…9'], effect: 'The sidebar entry by number, from the top' },
    { keys: ['Tab'], effect: 'Through the sidebar, then the panel; a board’s move list is one stop' },
  ];

  const mouse: { action: string; effect: string }[] = [
    { action: 'Drag a piece', effect: 'Move it; a longer drag takes a route along the path you drag' },
    { action: 'Drag in setup', effect: 'Swap two pieces' },
    { action: 'Click a shown step', effect: "Take it (the lobby's Settings, Hover: arrows or step mode)" },
    { action: 'Right-click a square', effect: 'Highlight it' },
    { action: 'Right-drag', effect: 'Draw an arrow' },
    { action: 'Shift / Ctrl or Alt', effect: 'With a right-click: red / blue instead of the usual color' },
    { action: 'Left-click', effect: 'Clear highlights and arrows' },
  ];
</script>

{#snippet body()}
  <div class="columns">
    {#each groups as g (g.group)}
      <section>
        <h3>{g.group}</h3>
        <dl>
          {#each g.shortcuts as s (s.id)}
            <dt>
              {#each s.keys as k, i (k)}{#if i > 0}<span class="or">or</span>{/if}<kbd>{keyName(k)}</kbd>{/each}
            </dt>
            <dd>{s.label}</dd>
          {/each}
        </dl>
      </section>
    {/each}
    <section>
      <h3>Lobby window</h3>
      <dl>
        {#each lobby as l (l.effect)}
          <dt>
            {#each l.keys as k, i (k)}{#if i > 0}<span class="or">or</span>{/if}<kbd>{k}</kbd>{/each}
          </dt>
          <dd>{l.effect}</dd>
        {/each}
      </dl>
    </section>
    <section class="wide">
      <h3>Mouse</h3>
      <dl>
        {#each mouse as m (m.action)}
          <dt>{m.action}</dt>
          <dd>{m.effect}</dd>
        {/each}
      </dl>
    </section>
  </div>
{/snippet}

{#if panel}
  <div class="screen">
    <header><h1 id="help-title">Help</h1></header>
    <div class="card">
      <h2>Keyboard and mouse</h2>
      <p class="note">
        The keys work in a board, as a panel here or in its own window; <kbd>?</kbd> there shows this too.
      </p>
      {@render body()}
    </div>
    <div class="card about">
      <h2>About Howdah</h2>
      <p class="version">
        Version {version ?? '…'}
        <button class="subtle" onclick={copyVersion} disabled={version == null}>{copied ? 'Copied' : 'Copy'}</button>
      </p>
      <p class="links">
        {#each [['Releases', `${REPO}/releases`], ['Changelog', `${REPO}/blob/main/CHANGELOG.md`], ['Source', REPO], ['Acknowledgements', `${REPO}#acknowledgements`]] as [label, url] (url)}
          <button class="link" onclick={() => api.openUrl(url).catch(() => {})}>{label}</button>
        {/each}
      </p>
      <p class="note">
        By Brian Haskin Jr., under the MIT license. The Howdah logo is by Brian Haskin Jr. and Claude. The classic board
        and pieces come from the original arimaa.com client, in the public domain, and the sounds are made from Kenney's
        CC0 samples.
      </p>
    </div>
  </div>
{:else}
  <dialog bind:this={dialog} onclose={onClose} {onkeydown} aria-labelledby="help-title">
    <h2 id="help-title">Keyboard and mouse</h2>
    {@render body()}
    <div class="buttons">
      <button class="primary" onclick={() => dialog?.close()}>Close</button>
    </div>
  </dialog>
{/if}

<style>
  dialog {
    width: min(720px, 92vw);
    max-height: 90vh;
  }
  h2 {
    margin: 0 0 8px;
    font-size: 16px;
  }
  header {
    display: flex;
    align-items: center;
    margin-bottom: 14px;
    min-height: 30px;
  }
  h1 {
    margin: 0;
    font-size: 20px;
  }
  .card {
    max-width: 900px;
  }
  .note {
    margin: 0 0 12px;
    font-size: 13px;
    color: var(--muted);
  }
  h3 {
    margin: 0 0 6px;
    font-size: 12px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--muted);
  }
  .columns {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
    gap: 12px 24px;
  }
  .wide {
    grid-column: 1 / -1;
  }
  section {
    padding-top: 8px;
    border-top: 1px solid var(--border);
  }
  dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 4px 12px;
    margin: 0;
    font-size: 13px;
  }
  dt {
    display: flex;
    align-items: center;
    gap: 4px;
    white-space: nowrap;
  }
  dd {
    margin: 0;
    color: var(--muted);
  }
  kbd {
    font:
      12px ui-monospace,
      'DejaVu Sans Mono',
      monospace;
    padding: 1px 5px;
    border: 1px solid var(--border);
    border-bottom-width: 2px;
    border-radius: 4px;
    background: var(--bg);
  }
  .or {
    font-size: 11px;
    color: var(--muted);
  }
  .about {
    margin-top: 12px;
  }
  .about p {
    margin: 0 0 8px;
  }
  .about .note {
    margin-bottom: 0;
  }
  .version {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .links {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 16px;
  }
  .buttons {
    display: flex;
    justify-content: flex-end;
    margin-top: 12px;
  }
</style>
