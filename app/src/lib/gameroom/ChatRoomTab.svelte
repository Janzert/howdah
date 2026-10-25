<script lang="ts">
  // The gameroom's chat room: Enter (public: the room and its archive show
  // who comes and goes), the log in the room's formatting, who's there, a
  // message box with the user's color, a preview of the message as the
  // room will show it, faces and pieces to put in it, and Leave.
  import { tick, untrack } from 'svelte';
  import { api } from '../api';
  import type { ChatEventView } from '../bindings/ChatEventView';
  import type { ChatMessageView } from '../bindings/ChatMessageView';
  import PieceGlyph from '@howdah/board/board/PieceGlyph.svelte';
  import { CHAT_PALETTE, chatTime, FACES, messageColor, parseHex, PIECE_SYMBOLS, SPOILER_COLOR } from '../chatroom';
  import { SQ } from '@howdah/board/geometry';
  import { settings } from '../settings.svelte';
  import { findTheme } from '@howdah/board/theme';
  import ChatLine from './ChatLine.svelte';
  import { chatroom } from './chatroom.svelte';
  import { darkMode } from './darkMode.svelte';
  import { gameroom } from './store.svelte';

  const view = $derived(chatroom.view);
  const roomState = $derived(view?.state ?? 'out');

  let text = $state('');
  let log: HTMLElement | undefined = $state();
  let input: HTMLInputElement | undefined = $state();
  let picking = $state(false);
  /** The user's own color as typed in the color popup. */
  let customText = $state('');
  let customInput: HTMLInputElement | undefined = $state();
  /** Whether Enter was pressed on what's typed (a color still being
   * typed isn't faulted until then). */
  let customDone = $state(false);
  const customParsed = $derived(parseHex(customText));
  const customProblem = $derived(
    !customText.trim()
      ? null
      : customParsed === SPOILER_COLOR
        ? "White is the room's spoiler tag; use Spoiler below"
        : customParsed || (!customDone && /^#?[0-9a-f]{0,5}$/i.test(customText.trim()))
          ? null
          : 'A color in hex, like #f70077',
  );
  /** Whether the faces and pieces are open. */
  let symbolsOpen = $state(false);
  let entry: HTMLElement | undefined = $state();
  const MAX_LENGTH = 2000;
  const theme = $derived(findTheme(settings.theme));

  // Asked more often while in view; what's shown counts as read.
  $effect(() => {
    const update = () => {
      api.setChatroomActive(!document.hidden).catch(() => {});
      if (!document.hidden) untrack(() => chatroom.markSeen());
    };
    update();
    document.addEventListener('visibilitychange', update);
    return () => {
      document.removeEventListener('visibilitychange', update);
      api.setChatroomActive(false).catch(() => {});
    };
  });

  // Keep to the bottom of the log as lines arrive, unless scrolled up.
  let stick = true;
  $effect.pre(() => {
    void view?.events.length;
    if (log) stick = log.scrollHeight - log.scrollTop - log.clientHeight < 40;
  });
  $effect(() => {
    void view?.events.length;
    if (log && stick) log.scrollTop = log.scrollHeight;
    if (!document.hidden) chatroom.markSeen();
  });

  /** Whether the message being written goes out as a spoiler. */
  let spoiler = $state(false);

  async function send(e: SubmitEvent) {
    e.preventDefault();
    const said = text;
    const hide = spoiler;
    if (!said.trim()) return;
    text = '';
    spoiler = false;
    symbolsOpen = false;
    // Kept if it didn't go and nothing new was typed meanwhile.
    if (!(await chatroom.say(said, hide)) && !text) {
      text = said;
      spoiler = hide;
    }
    input?.focus();
  }

  // The message being written, formatted by the backend as the room
  // will show it; a reply to older text is dropped.
  let draft = $state<ChatMessageView | null>(null);
  let asked = 0;
  $effect(() => {
    const said = text;
    const n = ++asked;
    if (!said.trim()) {
      draft = null;
      return;
    }
    api
      .formatChatMessage(said)
      .then((m) => {
        if (n === asked) draft = m;
      })
      .catch(() => {});
  });
  const preview = $derived<ChatEventView | null>(
    draft && text.trim()
      ? {
          id: -1,
          time: 0,
          user: view?.user ?? '',
          kind: 'message',
          color: spoiler ? SPOILER_COLOR : chatroom.color,
          emote: draft.emote,
          parts: draft.parts,
        }
      : null,
  );

  /** Takes the color dialog's choice as the user's own color, unless
   * it's the spoiler's white. */
  function pickCustom(el: HTMLInputElement) {
    const hex = parseHex(el.value);
    if (!hex) return;
    customText = `#${hex}`;
    customDone = true;
    if (hex === SPOILER_COLOR) return;
    chatroom.setCustom(hex);
    spoiler = false;
  }

  /** Puts `symbol` in the message where the cursor is (over what's
   * selected), the cursor after it. */
  async function insert(symbol: string) {
    const el = input;
    if (!el) return;
    const start = el.selectionStart ?? text.length;
    const end = el.selectionEnd ?? start;
    const next = text.slice(0, start) + symbol + text.slice(end);
    if (next.length > MAX_LENGTH) return;
    text = next;
    await tick();
    el.focus();
    el.setSelectionRange(start + symbol.length, start + symbol.length);
  }

  // A click outside the faces and pieces closes them.
  $effect(() => {
    if (!symbolsOpen) return;
    const close = (ev: PointerEvent) => {
      if (entry && !entry.contains(ev.target as Node)) symbolsOpen = false;
    };
    window.addEventListener('pointerdown', close);
    return () => window.removeEventListener('pointerdown', close);
  });

  const swatch = (hex: string) => messageColor(hex, darkMode.dark).color ?? 'var(--text)';
</script>

<div class="room">
  {#if roomState === 'out' || roomState === 'entering'}
    <section class="card" aria-labelledby="chat-out-title">
      <div class="card-head">
        <h2 id="chat-out-title">Chat room</h2>
        <span class="spacer"></span>
        {#if chatroom.count > 0}<span class="muted">{chatroom.count} in the room</span>{/if}
        <button onclick={() => gameroom.show('archive')} title="Earlier days of the room, kept here">Archive</button>
      </div>
      {#if view?.note}<p class="note" role="status">{view.note}</p>{/if}
      <p>The gameroom's chat, for everyone logged in to arimaa.com.</p>
      <p class="muted">
        Entering is public: the room shows who comes and goes, and it and every message stay in its archive.
        {#if settings.leaveChatForGames}
          You'll leave it when you play a rated game against a person (not postal) or an event game (Settings).
        {/if}
      </p>
      <div class="actions">
        <button class="primary" onclick={() => chatroom.enter()} disabled={chatroom.busy || roomState === 'entering'}>
          {roomState === 'entering' ? 'Entering…' : 'Enter the chat room'}
        </button>
      </div>
      {#if chatroom.error}<p class="error" role="alert">{chatroom.error}</p>{/if}
    </section>
  {:else}
    <div class="bar">
      <ul class="users" aria-label="In the chat room">
        {#each view?.users ?? [] as u (u)}
          <li class:me={u === view?.user}>{u}</li>
        {/each}
      </ul>
      <span class="spacer"></span>
      <button onclick={() => gameroom.show('archive')} title="Earlier days of the room, kept here">Archive</button>
      <button
        onclick={() => chatroom.leave()}
        disabled={chatroom.busy}
        title="Leave the chat room (the room shows you going out)">Leave the room</button
      >
    </div>
    {#if view?.note}<p class="note" role="status">{view.note}</p>{/if}
    <div class="log" bind:this={log} role="log" aria-label="Chat room" aria-live="polite">
      {#each view?.events ?? [] as e (e.id)}
        <ChatLine event={e} time={chatTime} />
      {:else}
        <p class="empty">Nothing said lately.</p>
      {/each}
    </div>
    {#if preview}
      <div class="preview" aria-label="Preview of your message">
        <ChatLine event={preview} time={chatTime} preview />
      </div>
    {/if}
    <form class="say" onsubmit={send}>
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="color"
        onkeydown={(k) => {
          if (k.key === 'Escape' && picking) {
            k.preventDefault();
            picking = false;
            input?.focus();
          }
        }}
      >
        <button
          type="button"
          class="swatch"
          class:spoiler-swatch={spoiler}
          style:background={spoiler ? undefined : swatch(chatroom.color)}
          onclick={() => {
            picking = !picking;
            customText = `#${chatroom.custom}`;
          }}
          aria-expanded={picking}
          title={spoiler ? 'The next message is a spoiler' : 'Your color'}
          aria-label={spoiler ? 'Color: spoiler, for the next message' : 'Your color'}
        ></button>
        {#if picking}
          <div class="palette" role="listbox" aria-label="Colors">
            <!-- The palette's last slot is the user's own color. -->
            {#each CHAT_PALETTE.slice(0, -1) as hex (hex)}
              <button
                type="button"
                role="option"
                aria-selected={hex === chatroom.color}
                aria-label="#{hex}"
                class="swatch"
                class:on={!spoiler && hex === chatroom.color}
                style:background={swatch(hex)}
                onclick={() => {
                  chatroom.setColor(hex);
                  spoiler = false;
                  picking = false;
                  input?.focus();
                }}
              ></button>
            {/each}
            <button
              type="button"
              role="option"
              aria-selected={chatroom.color === chatroom.custom}
              aria-label="Your own color, #{chatroom.custom}"
              title="Your own color: set it below"
              class="swatch custom-swatch"
              class:on={!spoiler && chatroom.color === chatroom.custom}
              style:background={swatch(chatroom.custom)}
              onclick={() => {
                chatroom.setColor(chatroom.custom);
                spoiler = false;
                customInput?.focus();
                customInput?.select();
              }}
            ></button>
            <div class="custom-option">
              <input
                title="Any color, in hex as on the room's page (#rrggbb)"
                bind:this={customInput}
                bind:value={customText}
                class:bad={customProblem}
                aria-label="Your own color in hex"
                aria-invalid={!!customProblem}
                spellcheck="false"
                autocomplete="off"
                maxlength="7"
                placeholder="#rrggbb"
                oninput={() => {
                  customDone = false;
                  if (customParsed && customParsed !== SPOILER_COLOR) {
                    chatroom.setCustom(customParsed);
                    spoiler = false;
                  }
                }}
                onkeydown={(k) => {
                  if (k.key === 'Enter') {
                    k.preventDefault();
                    customDone = true;
                    if (customProblem) return;
                    picking = false;
                    input?.focus();
                  }
                }}
              />
              <!-- The system's color dialog (GTK's, macOS's, or the
                   browser's), each with a way to type hex. -->
              <label class="choose" title="Choose your own color in the system's color dialog"
                >Choose…<input
                  type="color"
                  value="#{chatroom.custom}"
                  aria-label="Choose your own color"
                  oninput={(ev) => pickCustom(ev.currentTarget)}
                  onchange={(ev) => pickCustom(ev.currentTarget)}
                /></label
              >
            </div>
            {#if customProblem}<p class="custom-problem" role="alert">{customProblem}</p>{/if}
            <button
              type="button"
              role="option"
              aria-selected={spoiler}
              class="spoiler-option"
              class:on={spoiler}
              title="The next message in white, the room's spoiler tag: hidden until clicked here, and until selected on the room's page. Then back to your color."
              onclick={() => {
                spoiler = !spoiler;
                picking = false;
                input?.focus();
              }}><span class="swatch spoiler-swatch" aria-hidden="true"></span> Spoiler (next message)</button
            >
          </div>
        {/if}
      </div>
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="entry"
        bind:this={entry}
        onkeydown={(k) => {
          if (k.key === 'Escape' && symbolsOpen) {
            k.preventDefault();
            symbolsOpen = false;
            input?.focus();
          }
        }}
      >
        <input
          bind:this={input}
          bind:value={text}
          class:spoiler-input={spoiler}
          placeholder={spoiler ? 'A spoiler, hidden until clicked' : 'Say something to the room (/me for an action)'}
          aria-label="Message"
          autocomplete="off"
          maxlength={MAX_LENGTH}
        />
        <button
          type="button"
          class="symbols-toggle"
          class:on={symbolsOpen}
          onclick={() => (symbolsOpen = !symbolsOpen)}
          aria-expanded={symbolsOpen}
          aria-label="Faces and pieces"
          title="Faces and pieces">🙂</button
        >
        {#if symbolsOpen}
          <div class="symbols" role="group" aria-label="Faces and pieces">
            <div class="faces">
              {#each FACES as f (f.symbol)}
                <button
                  type="button"
                  title={f.symbol}
                  aria-label="{f.name} {f.symbol}"
                  onmousedown={(ev) => ev.preventDefault()}
                  onclick={() => insert(f.symbol)}>{f.emoji}</button
                >
              {/each}
            </div>
            <div class="pieces">
              {#each PIECE_SYMBOLS as p (p.symbol)}
                <button
                  type="button"
                  title="{p.color} {p.piece} {p.symbol}"
                  aria-label="{p.color} {p.piece} {p.symbol}"
                  onmousedown={(ev) => ev.preventDefault()}
                  onclick={() => insert(p.symbol)}
                  ><svg viewBox="0 0 {SQ} {SQ}" aria-hidden="true"
                    ><PieceGlyph piece={{ color: p.color, kind: p.piece }} {theme} /></svg
                  ></button
                >
              {/each}
            </div>
          </div>
        {/if}
      </div>
      <button type="submit" class="primary" disabled={!text.trim()}>Send</button>
    </form>
    {#if chatroom.error}<p class="error" role="alert">{chatroom.error}</p>{/if}
  {/if}
</div>

<style>
  .room {
    display: flex;
    flex-direction: column;
    gap: 10px;
    height: calc(100vh - 120px);
    min-height: 300px;
  }
  .room > .card {
    align-self: stretch;
  }
  .muted {
    color: var(--muted);
    font-size: 13px;
  }
  .note {
    margin: 0;
    padding: 6px 10px;
    border: 1px solid var(--border);
    border-radius: 8px;
    font-size: 13px;
  }
  .error {
    margin: 0;
    color: var(--warn);
    font-size: 13px;
  }
  .actions {
    margin-top: 10px;
  }
  .bar {
    display: flex;
    align-items: flex-start;
    gap: 8px;
  }
  .users {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin: 0;
    padding: 0;
    list-style: none;
    font-size: 13px;
  }
  .users li {
    padding: 1px 8px;
    border: 1px solid var(--border);
    border-radius: 10px;
  }
  .users li.me {
    font-weight: 600;
  }
  .log {
    flex: 1;
    overflow-y: auto;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--panel);
    font-size: 14px;
    line-height: 1.45;
    overflow-wrap: anywhere;
  }
  .say {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .entry {
    position: relative;
    flex: 1;
    display: flex;
  }
  .say input {
    flex: 1;
    padding-right: 34px;
  }
  .symbols-toggle {
    position: absolute;
    top: 50%;
    right: 3px;
    transform: translateY(-50%);
    width: 28px;
    height: 26px;
    padding: 0;
    border: none;
    border-radius: 6px;
    background: none;
    font-size: 17px;
    line-height: 1;
    filter: grayscale(0.7);
    opacity: 0.8;
  }
  .symbols-toggle:hover,
  .symbols-toggle.on {
    filter: none;
    opacity: 1;
    background: var(--hover);
  }
  .symbols {
    position: absolute;
    bottom: calc(100% + 6px);
    right: 0;
    z-index: 2;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 8px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--panel);
    box-shadow: 0 4px 14px rgb(0 0 0 / 0.2);
  }
  .faces {
    display: grid;
    grid-template-columns: repeat(7, 32px);
    gap: 4px;
  }
  .pieces {
    display: grid;
    grid-template-columns: repeat(6, 32px);
    gap: 4px;
    padding-top: 8px;
    border-top: 1px solid var(--border);
  }
  .symbols button {
    width: 32px;
    height: 32px;
    padding: 2px;
    border: 1px solid transparent;
    border-radius: 6px;
    background: none;
    font-size: 20px;
    line-height: 1;
  }
  .symbols button:hover,
  .symbols button:focus-visible {
    border-color: var(--border);
    background: var(--hover);
  }
  .symbols svg {
    width: 100%;
    height: 100%;
  }
  .preview {
    padding: 4px 10px;
    border: 1px dashed var(--border);
    border-radius: 8px;
    background: var(--panel);
    font-size: 14px;
    line-height: 1.45;
    overflow-wrap: anywhere;
    max-height: 7.5em;
    overflow-y: auto;
  }
  .say input.spoiler-input {
    border-style: dashed;
  }
  .swatch.spoiler-swatch {
    display: inline-block;
    flex: none;
    background: var(--muted);
    border: 1px dashed var(--text);
    vertical-align: middle;
  }
  .spoiler-option {
    grid-column: 1 / -1;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 3px 6px;
    font-size: 12px;
    white-space: nowrap;
  }
  .custom-swatch {
    box-shadow: inset 0 0 0 2px var(--panel);
    border-style: dashed;
    border-color: var(--text);
  }
  .custom-option {
    grid-column: 1 / -1;
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
  }
  .custom-option input {
    flex: 1;
    min-width: 0;
    padding: 2px 6px;
    font-family: ui-monospace, monospace;
    font-size: 12px;
  }
  .choose {
    position: relative;
    flex: none;
    padding: 2px 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    cursor: pointer;
    white-space: nowrap;
  }
  .choose:hover {
    background: var(--hover);
  }
  .choose:focus-within {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
  /* The color input opens the dialog; the label is what shows. */
  .choose input {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    opacity: 0;
    padding: 0;
    border: none;
    cursor: pointer;
  }
  .custom-option input.bad {
    border-color: var(--warn);
  }
  .custom-problem {
    grid-column: 1 / -1;
    margin: 0;
    color: var(--warn);
    font-size: 11px;
  }
  .spoiler-option.on {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
  .color {
    position: relative;
  }
  .swatch {
    width: 22px;
    height: 22px;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: 50%;
  }
  .swatch.on {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
  .palette {
    position: absolute;
    bottom: 30px;
    left: 0;
    z-index: 2;
    display: grid;
    grid-template-columns: repeat(6, 22px);
    gap: 6px;
    padding: 8px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--panel);
    box-shadow: 0 4px 14px rgb(0 0 0 / 0.2);
  }
</style>
