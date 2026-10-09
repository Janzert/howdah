<script lang="ts">
  import { keyName, shortcutFor, shortcutGroups } from './shortcuts';

  let { onClose }: { onClose: () => void } = $props();

  let dialog: HTMLDialogElement;
  const groups = shortcutGroups();

  $effect(() => {
    dialog.showModal();
  });

  // `?` closes the help as well as opening it (Esc closes any dialog).
  function onkeydown(e: KeyboardEvent) {
    if (shortcutFor(e)?.id === 'help') {
      e.preventDefault();
      dialog.close();
    }
  }

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

<dialog bind:this={dialog} onclose={onClose} {onkeydown} aria-labelledby="help-title">
  <h2 id="help-title">Keyboard and mouse</h2>
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
  <div class="buttons">
    <button class="primary" onclick={() => dialog.close()}>Close</button>
  </div>
</dialog>

<style>
  dialog {
    width: min(720px, 92vw);
    max-height: 90vh;
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
    margin: 0 0 8px;
    font-size: 16px;
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
    font: 12px ui-monospace, 'DejaVu Sans Mono', monospace;
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
  .buttons {
    display: flex;
    justify-content: flex-end;
    margin-top: 12px;
  }
</style>
