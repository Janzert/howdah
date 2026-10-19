<script lang="ts">
  // One event of the gameroom chat room's log, in the room's formatting:
  // its time, then a message (links, faces, piece symbols, /me, the
  // poster's color; white is a spoiler, its text hidden until clicked) or
  // someone coming or going. The chat room and its archive both use it,
  // and the chat room's preview of a message being written (no time, a
  // spoiler shown).
  import { api } from '../api';
  import type { ChatEventView } from '../bindings/ChatEventView';
  import PieceGlyph from '../board/PieceGlyph.svelte';
  import { FACE_EMOJI, messageColor } from '../chatroom';
  import { SQ } from '../geometry';
  import { settings } from '../settings.svelte';
  import { findTheme } from '../theme';
  import { darkMode } from './darkMode.svelte';

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
</script>

{#snippet parts()}{#each e.parts as p, i (i)}{#if p.type === 'text'}{p.text}{:else if p.type === 'link'}<a
        href={p.url}
        onclick={(ev) => {
          ev.preventDefault();
          ev.stopPropagation();
          api.openUrl(p.url).catch(() => {});
        }}>{p.url}</a
      >{:else if p.type === 'face'}<span class="face" title={p.symbol} role="img" aria-label={p.symbol}
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

<style>
  .line {
    display: flex;
    gap: 8px;
  }
  .time {
    flex: none;
    color: var(--muted);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
    padding-top: 1px;
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
  .face {
    font-style: normal;
  }
  .piece {
    width: 1.3em;
    height: 1.3em;
    vertical-align: -0.3em;
  }
</style>
