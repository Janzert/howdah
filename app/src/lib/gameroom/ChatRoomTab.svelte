<script lang="ts">
  // The gameroom's chat room: Enter (public: the room and its archive show
  // who comes and goes), the log in the room's formatting, who's there, a
  // message box with the user's color, and Leave.
  import { untrack } from 'svelte';
  import { api } from '../api';
  import { CHAT_PALETTE, chatTime, messageColor } from '../chatroom';
  import { settings } from '../settings.svelte';
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
    // Kept if it didn't go and nothing new was typed meanwhile.
    if (!(await chatroom.say(said, hide)) && !text) {
      text = said;
      spoiler = hide;
    }
    input?.focus();
  }

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
          onclick={() => (picking = !picking)}
          aria-expanded={picking}
          title={spoiler ? 'The next message is a spoiler' : 'Your color'}
          aria-label={spoiler ? 'Color: spoiler, for the next message' : 'Your color'}
        ></button>
        {#if picking}
          <div class="palette" role="listbox" aria-label="Colors">
            {#each CHAT_PALETTE as hex (hex)}
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
      <input
        bind:this={input}
        bind:value={text}
        class:spoiler-input={spoiler}
        placeholder={spoiler ? 'A spoiler, hidden until clicked' : 'Say something to the room (/me for an action)'}
        aria-label="Message"
        autocomplete="off"
        maxlength="2000"
      />
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
  .say input {
    flex: 1;
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
