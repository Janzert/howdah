<script lang="ts">
  // One event of the gameroom chat room's log, in the room's formatting:
  // its time, then a message (links, faces, piece symbols, /me, the
  // poster's color; white is a spoiler, its text hidden until clicked) or
  // someone coming or going. The chat room and its archive both use it,
  // and the chat room's preview of a message being written (no time, a
  // spoiler shown). A position link shows its final position and opens on a
  // board.
  import { api } from '../api';
  import type { ChatEventView } from '../bindings/ChatEventView';
  import type { ChatPartView } from '../bindings/ChatPartView';
  import MiniBoard from '@howdah/board/board/MiniBoard.svelte';
  import PieceGlyph from '@howdah/board/board/PieceGlyph.svelte';
  import { browserCanOpen, FACE_EMOJI, messageColor } from '../chatroom';
  import { SQ } from '@howdah/board/geometry';
  import { settings } from '../settings.svelte';
  import { findTheme } from '@howdah/board/theme';
  import { darkMode } from './darkMode.svelte';
  import { gameroom, savePref } from './store.svelte';

  let {
    event: e,
    time,
    preview = false,
  }: { event: ChatEventView; time: (unix: number) => string; preview?: boolean } = $props();

  const theme = $derived(findTheme(settings.theme));
  const look = $derived(messageColor(e.color, darkMode.dark));
  let revealed = $state(false);
  const shown = $derived(preview || revealed);
  const comings = { in: 'came in', out: 'went out', timeout: 'timed out', message: '' } as const;

  /** Shows or hides a spoiler again; not when the click ends a text
   * selection in it. */
  function toggle() {
    if (preview || window.getSelection()?.toString()) return;
    revealed = !revealed;
  }

  let menu = $state<{ url: string; x: number; y: number } | null>(null);

  function openMenu(ev: MouseEvent, url: string) {
    ev.preventDefault();
    ev.stopPropagation();
    menu = { url, x: ev.clientX, y: ev.clientY };
  }

  /** Opens a link's position, game or puzzle on a board; otherwise it's
   * for the browser. */
  function openLink(ev: MouseEvent, p: Extract<ChatPartView, { type: 'link' }>) {
    ev.preventDefault();
    ev.stopPropagation();
    if (p.plan) {
      // A new board starts with the last view, so this is the link's view.
      savePref('flipped', p.plan.flipped ? '1' : '0');
      gameroom.enter((a) => a.loadGame(p.plan!.record));
    }
    else if (p.game) gameroom.open(p.game);
    else if (p.puzzle) gameroom.enter((a) => a.openPuzzle(p.puzzle!), undefined, 'Loading the puzzle…');
    else api.openUrl(p.url).catch(() => {});
  }

  // Listening only while the menu is open: a log can have thousands of lines.
  $effect(() => {
    if (!menu) return;
    const close = () => (menu = null);
    const escape = (k: KeyboardEvent) => {
      if (k.key === 'Escape') close();
    };
    window.addEventListener('click', close);
    window.addEventListener('contextmenu', close, true);
    window.addEventListener('keydown', escape, true);
    return () => {
      window.removeEventListener('click', close);
      window.removeEventListener('contextmenu', close, true);
      window.removeEventListener('keydown', escape, true);
    };
  });
</script>

{#snippet parts()}{#each e.parts as p, i (i)}{#if p.type === 'text'}{p.text}{:else if p.type === 'link'}<a
        href={p.url}
        oncontextmenu={(ev) => openMenu(ev, p.url)}
        onclick={(ev) => openLink(ev, p)}>{p.url}</a
      >{#if p.plan}{@const plan = p.plan}<button
          class="plan"
          title="Open on a board"
          onclick={(ev) => openLink(ev, p)}
          oncontextmenu={(ev) => openMenu(ev, p.url)}
          onkeydown={(k) => k.stopPropagation()}
          ><span class="mini"><MiniBoard position={plan.position} {theme} flipped={plan.flipped} /></span
          >{#if plan.knowsSide}<span class="side"
              >{plan.position.sideToMove === 'gold' ? 'Gold' : 'Silver'} to move</span
            >{/if}</button
        >{/if}{:else if p.type === 'face'}<span class="face" title={p.symbol} role="img" aria-label={p.symbol}
        >{FACE_EMOJI[p.name] ?? p.symbol}</span
      >{:else}<svg class="piece" viewBox="0 0 {SQ} {SQ}" role="img" aria-label="{p.color} {p.piece}"
        ><title>{p.color} {p.piece}</title><PieceGlyph piece={{ color: p.color, kind: p.piece }} {theme} /></svg
      >{/if}{/each}{/snippet}

<div class="line" class:coming={e.kind !== 'message'}>
  {#if preview}<span class="time">Preview</span>{:else}<time
      class="time"
      datetime={new Date(e.time * 1000).toISOString()}>{time(e.time)}</time
    >{/if}
  {#if e.kind === 'message'}
    <span class="msg" class:emote={e.emote} style:color={look.color}>
      <span class="name">{e.user}</span>{#if !e.emote}{' '}{/if}{#if look.spoiler}<span
          class="spoiler"
          class:hidden={!shown}
          role="button"
          tabindex="0"
          aria-pressed={shown}
          aria-label={shown ? undefined : 'Spoiler, hidden'}
          title={preview
            ? 'Spoiler: hidden in the room until clicked'
            : shown
              ? 'Spoiler: click to hide'
              : 'Spoiler: click to show'}
          onclick={toggle}
          onkeydown={(k) => {
            if (k.key === 'Enter' || k.key === ' ') {
              k.preventDefault();
              toggle();
            }
          }}>{@render parts()}</span
        >{:else}{@render parts()}{/if}
    </span>
  {:else}
    <span><span class="name">{e.user}</span> {comings[e.kind]}</span>
  {/if}
</div>

{#if menu}
  {@const url = menu.url}
  <div class="menu" role="menu" style="left: {menu.x}px; top: {menu.y}px">
    <button role="menuitem" onclick={() => navigator.clipboard.writeText(url)}>Copy link</button>
    {#if browserCanOpen(url)}
      <button role="menuitem" onclick={() => api.openUrl(url).catch(() => {})}>Open in browser</button>
    {/if}
  </div>
{/if}

<style>
  .line {
    display: flex;
    gap: 8px;
  }
  .time {
    flex: none;
    color: var(--muted);
    font-size: 0.86em;
    font-variant-numeric: tabular-nums;
    padding-top: 0.08em;
  }
  .coming {
    color: var(--muted);
    font-style: italic;
  }
  .name {
    font-weight: 600;
  }
  .msg.emote {
    font-style: italic;
  }
  .spoiler {
    cursor: pointer;
    padding: 0 3px;
    border-radius: 3px;
    outline: 1px dashed var(--muted);
    outline-offset: 0;
  }
  .spoiler.hidden {
    background: var(--muted);
    color: transparent;
  }
  .spoiler.hidden * {
    visibility: hidden;
  }
  .plan {
    display: block;
    width: fit-content;
    margin: 4px 0;
    padding: 0;
    border: none;
    background: none;
    color: var(--muted);
    font-size: 12px;
    font-style: normal;
    cursor: pointer;
  }
  .mini {
    display: block;
    width: 160px;
    height: 160px;
    margin-bottom: 2px;
  }
  .menu {
    position: fixed;
    z-index: 50;
    display: flex;
    flex-direction: column;
    min-width: 150px;
    padding: 4px;
    background: var(--panel);
    color: var(--text);
    border: 1px solid var(--border);
    border-radius: 6px;
    box-shadow: 0 4px 16px rgb(0 0 0 / 0.3);
    font-size: 13px;
  }
  .menu button {
    padding: 5px 10px;
    text-align: left;
    border: 0;
    border-radius: 4px;
    background: none;
    color: inherit;
    font: inherit;
    cursor: pointer;
  }
  .menu button:hover {
    background: var(--hover);
  }
  .face {
    font-style: normal;
  }
  .piece {
    width: 1.3em;
    height: 1.3em;
    vertical-align: -0.3em;
  }
</style>
